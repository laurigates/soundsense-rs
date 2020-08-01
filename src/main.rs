#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![cfg_attr(debug_assertions, windows_subsystem = "console")]

mod message;
mod sound;
mod ui;
mod util;

use crate::util::{
    event::{Event, Events},
    StatefulList,
};

#[macro_use]
extern crate log;
use crate::message::{SoundMessage, UIMessage};
use crossbeam::channel::unbounded as channel;
use crossbeam::channel::{Receiver, Sender};

use regex::Regex;
use std::{env, error::Error, fs, io, path::PathBuf, sync::Mutex};
use termion::{event::Key, input::MouseTerminal, raw::IntoRawMode, screen::AlternateScreen};
use tui::{
    backend::Backend,
    backend::TermionBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Gauge, List, Text},
    Frame, Terminal,
};

/// How SoundSense-RS works:
/// 1. Dwarf Fortress(&DFHack) writes into gamelog.txt
/// 2. In the Sound thread, every loop, the SoundManager reads the newly written lines.
/// 3. The SoundManager iterates through the SoundEntries, and checks if any of their patterns match.
/// 4. If a pattern matches, play the SoundEntry's SoundFiles on the appropriate SoundChannel.
///
/// All the while the UI thread handles user input and sends SoundMessage to the SoundThread
/// through a Sender<SoundMessage>, while the Sound thread sends UIMessages to the UI through
/// a Sender<UIMessage>.

struct Channel {
    name: String,
    volume: f64,
    paused: bool,
}

impl Channel {
    fn new(name: String, volume: f64) -> Channel {
        Channel {
            name,
            volume,
            paused: false,
        }
    }
}

struct App {
    pub should_quit: bool,
    sound_tx: Sender<SoundMessage>,
    ui_rx: Receiver<UIMessage>,
    channels: StatefulList<Channel>,
    items: Vec<String>,
}

impl App {
    fn new(sound_tx: Sender<SoundMessage>, ui_rx: Receiver<UIMessage>) -> App {
        App {
            should_quit: false,
            sound_tx,
            ui_rx,
            channels: StatefulList::new(),
            items: Vec::new(),
        }
    }

    pub fn on_up(&mut self) {
        self.channels.previous()
    }

    pub fn on_down(&mut self) {
        self.channels.next()
    }

    pub fn on_right(&mut self) {
        match self.channels.state.selected() {
            Some(i) => {
                if self.channels.items[i].volume < 100.0 {
                    self.channels.items[i].volume += 1.0;
                }
                let channel_name: Box<str> = self.channels.items[i].name.to_string().into();
                let channel_volume: f32 = self.channels.items[i].volume as f32;
                self.sound_tx
                    .send(SoundMessage::VolumeChange(channel_name, channel_volume))
                    .unwrap();
            }
            None => (),
        }
        self.save_config()
    }

    pub fn on_left(&mut self) {
        match self.channels.state.selected() {
            Some(i) => {
                if self.channels.items[i].volume >= 1.0 {
                    self.channels.items[i].volume += -1.0;
                }
                let channel_name: Box<str> = self.channels.items[i].name.to_string().into();
                let channel_volume: f32 = self.channels.items[i].volume as f32;
                self.sound_tx
                    .send(SoundMessage::VolumeChange(channel_name, channel_volume))
                    .unwrap();
            }
            None => (),
        }
        self.save_config()
    }

    fn save_config(&mut self) {
        // Save volumes to config file before quitting
        let mut conf_path = dirs::config_dir().expect("Failed to get configuration directory.");
        conf_path.push("soundsense-rs");
        if !conf_path.is_dir() {
            fs::create_dir(&conf_path).expect("Failed to create soundsense-rs config directory.");
        }
        conf_path.push("default-volumes.ini");
        let conf_file =
            fs::File::create(conf_path).expect("Failed to create default-volumes.ini file.");

        self.sound_tx
            .send(SoundMessage::SetCurrentVolumesAsDefault(conf_file))
            .unwrap();
    }

    pub fn on_key(&mut self, c: char) {
        match c {
            'q' => {
                self.should_quit = true;
            }
            ' ' => {
                // Pause selected channel
                match self.channels.state.selected() {
                    Some(i) => {
                        let channel_name: Box<str> = self.channels.items[i].name.to_string().into();
                        self.sound_tx
                            .send(SoundMessage::PlayPause(channel_name))
                            .unwrap();
                    }
                    None => (),
                }
            }
            _ => {}
        }
    }
    fn update(&mut self) {
        for ui_message in self.ui_rx.try_iter() {
            match ui_message {
                UIMessage::LoadedSoundpack(channel_names) => {
                    for channel in &channel_names {
                        let new_channel = Channel::new(channel.to_string(), 0.0);
                        self.channels.items.push(new_channel);
                    }

                    // Select the first channel by default
                    self.channels.state.select(Some(0));

                    let value = format!(
                        "Soundpack loaded! Loaded channels: {}.",
                        &channel_names.join(", ")
                    );
                    self.items.push(value)
                }
                UIMessage::LoadedVolumeSettings(entries) => {
                    for (name, volume) in &entries {
                        self.items.push(format!("{}: {}", name, volume));
                        for channel in &mut self.channels.items {
                            if channel.name == name.to_string() {
                                channel.volume = *volume as f64;
                            }
                        }
                    }
                }
                UIMessage::LoadedGamelog => {
                    let value = format!("Gamelog loaded!");
                    self.items.push(value)
                }
                UIMessage::LoadedIgnoreList => {
                    let value = format!("Ignore list loaded!");
                    self.items.push(value)
                }
                UIMessage::ChannelWasPlayPaused(name, is_paused) => {
                    let value = format!("Channel {} is_paused: {}.", &name, is_paused);
                    for channel in &mut self.channels.items {
                        if channel.name == name.to_string() {
                            channel.paused = !channel.paused;
                        }
                    }
                    self.items.push(value)
                }
                UIMessage::SoundThreadPanicked(name, text) => {
                    let value = format!("Error: {} {}", &name, &text);
                    self.items.push(value)
                }
            }
        }
    }

    fn draw<B: Backend>(&mut self, f: &mut Frame<B>) {
        let block = Block::default()
            .borders(Borders::ALL)
            .title("soundsense-rs");
        f.render_widget(block, f.size());

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .margin(1)
            .constraints([Constraint::Percentage(40), Constraint::Percentage(60)].as_ref())
            .split(f.size());

        {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .margin(1)
                .constraints(
                    [
                        Constraint::Ratio(1, 6),
                        Constraint::Ratio(1, 6),
                        Constraint::Ratio(1, 6),
                        Constraint::Ratio(1, 6),
                        Constraint::Ratio(1, 6),
                        Constraint::Ratio(1, 6),
                    ]
                    .as_ref(),
                )
                .split(chunks[0]);

            for (i, channel) in self.channels.items.iter().enumerate() {
                let mut color = Color::LightGreen;
                // Hightlight selected item
                if self.channels.state.selected() == Some(i) {
                    color = Color::Red
                }
                let mut channel_label = channel.name.to_string();
                if channel.paused == true {
                    channel_label.push_str("(paused)")
                }
                let gauge = Gauge::default()
                    .style(Style::default().fg(color).bg(Color::Black))
                    .label(&channel_label)
                    .percent(channel.volume as u16);
                f.render_widget(gauge, chunks[i]);
            }
        }
        {
            let chunks = Layout::default()
                .direction(Direction::Horizontal)
                .margin(0)
                .constraints([Constraint::Percentage(100)].as_ref())
                .split(chunks[1]);

            let items = self.items.iter().map(|i| Text::raw(i));
            let items = List::new(items)
                .block(Block::default().borders(Borders::ALL).title("List"))
                .style(Style::default().fg(Color::Green))
                .highlight_style(
                    Style::default()
                        .fg(Color::LightGreen)
                        .modifier(Modifier::BOLD),
                );
            f.render_widget(items, chunks[0])
        }
    }
}

fn main() -> Result<(), Box<dyn Error>> {
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
    let sound_thread = std::thread::Builder::new()
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

    // Terminal initialization
    let stdout = io::stdout().into_raw_mode()?;
    let stdout = MouseTerminal::from(stdout);
    let stdout = AlternateScreen::from(stdout);
    let backend = TermionBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.hide_cursor()?;

    let mut app = App::new(sound_tx, ui_rx);
    let events = Events::new();

    loop {
        terminal.draw(|mut f| app.draw(&mut f))?;

        match events.next()? {
            Event::Input(key) => match key {
                Key::Char(c) => {
                    app.on_key(c);
                }
                Key::Up => {
                    app.on_up();
                }
                Key::Down => {
                    app.on_down();
                }
                Key::Left => {
                    app.on_left();
                }
                Key::Right => {
                    app.on_right();
                }
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
