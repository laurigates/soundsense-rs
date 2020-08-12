#[allow(dead_code)]
mod util;

mod app;
mod message;
mod sound;
mod ui;

use app::App;
use argh::FromArgs;

#[macro_use]
extern crate num_derive;
extern crate num_traits;

#[macro_use]
extern crate log;
use crate::message::SoundMessage;
use crossbeam::channel::unbounded as channel;

use std::{
    error::Error,
    io::{stdout, Write},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event as CEvent, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};

enum Event<I> {
    Input(I),
    Tick,
}

/// Crossterm demo
#[derive(Debug, FromArgs)]
struct Cli {
    /// time in ms between two ticks.
    #[argh(option, default = "250")]
    tick_rate: u64,
    /// whether unicode symbols are used to improve the overall look of the app
    #[argh(option, default = "true")]
    enhanced_graphics: bool,
}

use tui::{backend::CrosstermBackend, Terminal};

use regex::Regex;
use std::{env, io, path::PathBuf, sync::Mutex};

/// How SoundSense-RS works:
/// 1. Dwarf Fortress(&DFHack) writes into gamelog.txt
/// 2. In the Sound thread, every loop, the SoundManager reads the newly written lines.
/// 3. The SoundManager iterates through the SoundEntries, and checks if any of their patterns match.
/// 4. If a pattern matches, play the SoundEntry's SoundFiles on the appropriate SoundChannel.
///
/// All the while the UI thread handles user input and sends SoundMessage to the SoundThread
/// through a Sender<SoundMessage>, while the Sound thread sends UIMessages to the UI through
/// a Sender<UIMessage>.

fn main() -> Result<(), Box<dyn Error>> {
    let cli: Cli = argh::from_env();
    // Setup and initialize the env_logger.
    let env = env_logger::Env::default()
        .filter_or("SOUNDSENSE_RS_LOG", "warn")
        .write_style_or("SOUNDSENSE_RS_LOG_STYLE", "always");
    env_logger::Builder::from_env(env)
        .format_module_path(false)
        .format_timestamp_millis()
        .init();
    info!("Starting SoundSense-RS");

    // Setup getopts style argument handling.
    let args: Vec<String> = env::args().collect();
    let mut opts = getopts::Options::new();

    opts.optopt(
        "l",
        "gamelog",
        "Path to the gamelog.txt file. (Default: .\\gamelog.txt)",
        "LOG_FILE",
    )
    .optopt(
        "p",
        "soundpack",
        "Path to the soundpack directory. (Default: .\\soundpack)",
        "PACK_DIR",
    )
    .optopt(
        "i",
        "ignore",
        "Path to the ignore.txt file. (Default: .\\ignore.txt)",
        "IGNORE_FILE",
    )
    .optflag(
        "",
        "no-config",
        "Don't read config files on start. Will use the given paths, or soundsense-rs defaults.",
    )
    .optflag("", "cli", "Use the command line interface.");

    // If there are errors in the arguments, print the usage of SoundSense-RS and quit.
    let matches = match opts.parse(&args[1..]) {
        Ok(matches) => matches,
        Err(e) => {
            error!("{}", e);
            println!("{}", opts.usage("SoundSense-RS"));
            return Ok(());
        }
    };

    // Check if there are config files available.
    // If so, read `soundsense-rs/default-paths.ini`.
    let config = if !matches.opt_present("no-config") {
        dirs::config_dir()
            .map(|mut p| {
                p.push("soundsense-rs/default-paths.ini");
                debug!("Checking for default-path config in: {}", p.display());
                p
            })
            .filter(|p| p.is_file())
            .or_else(|| {
                env::current_exe()
                    .ok()
                    .map(|mut p| {
                        p.pop();
                        p.push("default-paths.ini");
                        debug!("Checking for default-path config in: {}", p.display());
                        p
                    })
                    .filter(|p| p.is_file())
            })
            .and_then(|p| std::fs::read_to_string(p).ok())
    } else {
        None
    };

    let gamelog_path = matches
        .opt_str("l")
        // If a path is given, and is a file, use that as the gamelog.
        .and_then(|path| {
            let path = PathBuf::from(path);
            if path.is_file() {
                Some(path)
            } else {
                None
            }
        })
        // Else if config file contains path to the gamelog, use that as the gamelog.
        .or_else(|| {
            config.as_ref().and_then(|config_txt| {
                Regex::new("gamelog=(.+)")
                    .unwrap()
                    .captures(&config_txt)
                    .and_then(|c| c.get(1))
                    .map(|m| PathBuf::from(m.as_str()))
                    .filter(|p| p.is_file())
            })
        })
        // Else try to find `gamelog.txt` in the current working directory.
        // Otherwise, just return None.
        .or_else(|| {
            let mut path = env::current_dir().expect("Error finding current working directory.");
            path.push("gamelog.txt");
            if path.is_file() {
                Some(path)
            } else {
                None
            }
        });
    let soundpack_path = matches
        .opt_str("p")
        // If a path is given, and is a directory, use that as the soundpack.
        .and_then(|path| {
            let path = PathBuf::from(path);
            if path.is_dir() {
                Some(path)
            } else {
                None
            }
        })
        // Else if config file contains path to the soundpack, use that as the soundpack.
        .or_else(|| {
            config.as_ref().and_then(|config_txt| {
                Regex::new("soundpack=(.+)")
                    .unwrap()
                    .captures(&config_txt)
                    .and_then(|c| c.get(1))
                    .map(|m| PathBuf::from(m.as_str()))
                    .filter(|p| p.is_dir())
            })
        })
        // Else try to find `soundpack` directory in the current working directory.
        // Otherwise, just return None.
        .or_else(|| {
            let mut path = env::current_dir().expect("Error finding current working directory.");
            path.push("soundpack");
            if path.is_dir() {
                Some(path)
            } else {
                None
            }
        });
    let ignore_path = matches
        .opt_str("i")
        // If a path is given, and is a file, use that as the ignore list.
        .and_then(|path| {
            let path = PathBuf::from(path);
            if path.is_file() {
                Some(path)
            } else {
                None
            }
        })
        // Else if config file contains path to the ignore list, use that as the ignore list.
        .or_else(|| {
            config.as_ref().and_then(|config_txt| {
                Regex::new("ignore=(.+)")
                    .unwrap()
                    .captures(&config_txt)
                    .and_then(|c| c.get(1))
                    .map(|m| PathBuf::from(m.as_str()))
                    .filter(|p| p.is_file())
            })
        })
        // Else try to find `ignore.txt` in the current working directory.
        // Otherwise, just return None.
        .or_else(|| {
            let mut path = env::current_dir().expect("Error finding current working directory.");
            path.push("ignore.txt");
            if path.is_file() {
                Some(path)
            } else {
                None
            }
        });

    let (sound_tx, sound_rx) = channel();
    let (ui_tx, ui_rx) = channel();

    // Build and spawn the Sound thread.
    std::thread::Builder::new()
        .name("sound_thread".to_string())
        .spawn(move || sound::run(sound_rx, ui_tx))
        .unwrap();

    if let Some(path) = &soundpack_path {
        sound_tx
            .send(SoundMessage::ChangeSoundpack(path.clone()))
            .unwrap();
    }
    if let Some(path) = &gamelog_path {
        sound_tx
            .send(SoundMessage::ChangeGamelog(path.clone()))
            .unwrap();
    }
    if let Some(path) = &ignore_path {
        sound_tx
            .send(SoundMessage::ChangeIgnoreList(path.clone()))
            .unwrap();
    }

    let gamelog_path = Mutex::new(gamelog_path);
    let soundpack_path = Mutex::new(soundpack_path);
    let ignore_path = Mutex::new(ignore_path);

    if let Some(path) = gamelog_path.lock().unwrap().as_ref() {
        println!("gamelog={}", path.to_string_lossy());
    };
    if let Some(path) = soundpack_path.lock().unwrap().as_ref() {
        println!("soundpack={}", path.to_string_lossy());
    };
    if let Some(path) = ignore_path.lock().unwrap().as_ref() {
        println!("ignore={}", path.to_string_lossy());
    };

    enable_raw_mode()?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;

    let backend = CrosstermBackend::new(stdout);

    let mut terminal = Terminal::new(backend)?;

    // Setup input handling
    let (tx, rx) = mpsc::channel();

    let tick_rate = Duration::from_millis(cli.tick_rate);

    thread::spawn(move || {
        let mut last_tick = Instant::now();
        loop {
            // poll for tick rate duration, if no events, sent tick event.
            if event::poll(tick_rate - last_tick.elapsed()).unwrap() {
                if let CEvent::Key(key) = event::read().unwrap() {
                    tx.send(Event::Input(key)).unwrap();
                }
            }
            if last_tick.elapsed() >= tick_rate {
                tx.send(Event::Tick).unwrap();
                last_tick = Instant::now();
            }
        }
    });

    let mut app = App::new(sound_tx, ui_rx);

    terminal.clear()?;

    loop {
        terminal.draw(|mut f| ui::draw(&app, &mut f))?;

        match rx.recv()? {
            Event::Input(event) => match event.code {
                KeyCode::Char('q') => {
                    disable_raw_mode()?;
                    execute!(
                        terminal.backend_mut(),
                        LeaveAlternateScreen,
                        DisableMouseCapture
                    )?;
                    terminal.show_cursor()?;
                    break;
                }
                KeyCode::Char(c) => app.on_key(c),
                KeyCode::Left => app.on_left(),
                KeyCode::Up => app.on_up(),
                KeyCode::Right => app.on_right(),
                KeyCode::Down => app.on_down(),
                _ => {}
            },
            Event::Tick => {
                app.update();
            }
        }
        if app.should_quit {
            break;
        }
    }
    Ok(())
}
