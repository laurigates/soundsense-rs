use crate::message::{SoundMessage, Threshold, UIMessage};
use crate::util::StatefulList;
use crossbeam::channel::{Receiver, Sender};
use std::fs;

pub struct Channel {
    pub name: String,
    pub volume: f64,
    pub paused: bool,
    pub threshold: Threshold,
}

impl Channel {
    fn new(name: String, volume: f64) -> Channel {
        Channel {
            name,
            volume,
            paused: false,
            threshold: Threshold::Everything,
        }
    }
}

pub struct App {
    pub should_quit: bool,
    sound_tx: Sender<SoundMessage>,
    ui_rx: Receiver<UIMessage>,
    pub channels: StatefulList<Channel>,
    pub items: Vec<String>,
}

impl App {
    pub fn new(sound_tx: Sender<SoundMessage>, ui_rx: Receiver<UIMessage>) -> App {
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
        if let Some(i) = self.channels.state.selected() {
            if self.channels.items[i].volume < 100.0 {
                self.channels.items[i].volume += 1.0;
            }
            let channel_name: Box<str> = self.channels.items[i].name.to_string().into();
            let channel_volume: f32 = self.channels.items[i].volume as f32;
            self.sound_tx
                .send(SoundMessage::VolumeChange(channel_name, channel_volume))
                .unwrap();
        }
        self.save_config()
    }

    pub fn on_left(&mut self) {
        if let Some(i) = self.channels.state.selected() {
            if self.channels.items[i].volume >= 1.0 {
                self.channels.items[i].volume += -1.0;
            }
            let channel_name: Box<str> = self.channels.items[i].name.to_string().into();
            let channel_volume: f32 = self.channels.items[i].volume as f32;
            self.sound_tx
                .send(SoundMessage::VolumeChange(channel_name, channel_volume))
                .unwrap();
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
            's' => {
                // Skip on selected channel
                if let Some(i) = self.channels.state.selected() {
                    let channel_name: Box<str> = self.channels.items[i].name.to_string().into();
                    self.sound_tx
                        .send(SoundMessage::SkipCurrentSound(channel_name))
                        .unwrap();
                }
            }
            't' => {
                // Cycle selected channel threshold
                if let Some(i) = self.channels.state.selected() {
                    let channel_name: Box<str> = self.channels.items[i].name.to_string().into();
                    self.channels.items[i].threshold =
                        Threshold::next_threshold(self.channels.items[i].threshold);
                    self.sound_tx
                        .send(SoundMessage::ThresholdChange(
                            channel_name,
                            self.channels.items[i].threshold,
                        ))
                        .unwrap();
                }
            }
            ' ' => {
                // Pause selected channel
                if let Some(i) = self.channels.state.selected() {
                    let channel_name: Box<str> = self.channels.items[i].name.to_string().into();
                    self.sound_tx
                        .send(SoundMessage::PlayPause(channel_name))
                        .unwrap();
                }
            }
            _ => (),
        }
    }

    pub fn update(&mut self) {
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

                        if let Some(channel) = self
                            .channels
                            .items
                            .iter_mut()
                            .find(|x| x.name == name.to_string())
                        {
                            channel.volume = *volume as f64;
                        }
                    }
                }
                UIMessage::LoadedGamelog => {
                    let value = "Gamelog loaded!".to_string();
                    self.items.push(value)
                }
                UIMessage::LoadedIgnoreList => {
                    let value = "Ignore list loaded!".to_string();
                    self.items.push(value)
                }
                UIMessage::ChannelSoundWasSkipped(name) => {
                    let log_message = match self
                        .channels
                        .items
                        .iter()
                        .find(|&x| x.name == name.to_string())
                    {
                        Some(channel) => format!("Channel {} sound skipped.", channel.name),
                        None => "Channel could not be found when trying to skip sound.".to_string(),
                    };
                    self.items.push(log_message)
                }
                UIMessage::ChannelWasPlayPaused(name, is_paused) => {
                    let log_message = match self
                        .channels
                        .items
                        .iter_mut()
                        .find(|x| x.name == name.to_string())
                    {
                        Some(channel) => {
                            channel.paused = !channel.paused;
                            format!("Channel {} is paused: {}.", channel.name, is_paused)
                        }
                        None => {
                            "Channel could not be found when trying to pause channel.".to_string()
                        }
                    };
                    self.items.push(log_message)
                }
                UIMessage::ChannelThresholdWasChanged(name, threshold) => {
                    let log_message = match self
                        .channels
                        .items
                        .iter_mut()
                        .find(|x| x.name == name.to_string())
                    {
                        Some(channel) => format!(
                            "Channel {} threshold was changed to {}.",
                            channel.name, threshold
                        ),
                        None => "Channel could not be found when trying to change threshold."
                            .to_string(),
                    };
                    self.items.push(log_message)
                }
                UIMessage::SoundThreadPanicked(name, text) => {
                    let value = format!("Error: {} {}", &name, &text);
                    self.items.push(value)
                }
            }
        }
    }
}
