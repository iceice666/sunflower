use rand::prelude::*;
use rodio::{OutputStream, Sink};
use std::{collections::VecDeque, fmt::Debug};
use tracing::{debug, warn};

use crate::{error::PlayerResult, TrackObject};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RepeatState {
    Queue,
    Track,
    None,
}

pub struct Player {
    playlist: VecDeque<TrackObject>,
    current_track: Option<TrackObject>,
    sink: Sink,
    __stream: OutputStream,

    // Flags
    repeat: RepeatState,
    is_shuffle: bool,
}

impl Debug for Player {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Player")
            .field("playlist", &self.playlist)
            .field("current_track", &self.current_track)
            .field("repeat", &self.repeat)
            .field("is_shuffle", &self.is_shuffle)
            .finish()
    }
}

impl Player {
    pub fn try_new() -> PlayerResult<Self> {
        let (__stream, stream_handle) = OutputStream::try_default()?;
        let sink = Sink::try_new(&stream_handle)?;

        Ok(Self {
            playlist: VecDeque::new(),
            current_track: None,
            sink,
            __stream,

            repeat: RepeatState::None,
            is_shuffle: false,
        })
    }

    #[inline]
    pub fn add_track(&mut self, track: TrackObject) {
        self.playlist.push_back(track);
    }

    #[tracing::instrument]
    pub fn mainloop(mut self) -> PlayerResult {
        debug!("Starting mainloop");
        while !self.playlist.is_empty() {
            self.update_current_track();

            let Some(ref mut track) = self.current_track else {
                continue;
            };
            debug!("Next track: {}", track.get_unique_id());

            debug!("Building source");
            let source = match track.build_source() {
                Ok(source) => source,
                Err(e) => {
                    warn!(
                        "Failed to build source for track {}: {}",
                        track.get_unique_id(),
                        e
                    );
                    continue;
                }
            };

            debug!("Appending source to sink");
            self.sink.append(source);

            debug!("Waiting for track to end");
            self.sink.sleep_until_end();
        }

        Ok(())
    }

    #[inline]
    fn update_current_track(&mut self) {
        if self.repeat == RepeatState::Track && self.current_track.is_some() {
            // nop
        } else if self.repeat == RepeatState::Queue && self.current_track.is_some() {
            self.playlist.push_back(self.current_track.take().unwrap());
            self.current_track = self.playlist.pop_front();
        } else if self.is_shuffle {
            let mut rng = rand::thread_rng();
            let index = rng.gen_range(0..self.playlist.len());
            self.playlist.swap(0, index);
            self.current_track = self.playlist.pop_front();
        } else {
            self.current_track = self.playlist.pop_front();
        }
    }

    #[inline]
    pub fn refresh_sink(&mut self) -> PlayerResult {
        self.sink.stop();
        let (__stream, stream_handle) = OutputStream::try_default()?;
        let sink = Sink::try_new(&stream_handle)?;

        self.sink = sink;
        self.__stream = __stream;

        Ok(())
    }

    // Interface methods

    #[inline]
    pub fn set_volume(&mut self, volume: f32) {
        self.sink.set_volume(volume);
    }

    #[inline]
    pub fn get_volume(&self) -> f32 {
        self.sink.volume()
    }

    #[inline]
    pub fn toggle_shuffle(&mut self) {
        self.is_shuffle = !self.is_shuffle;
    }

    #[inline]
    pub fn toggle_repeat(&mut self) {
        match self.repeat {
            RepeatState::None => self.repeat = RepeatState::Queue,
            RepeatState::Queue => self.repeat = RepeatState::Track,
            RepeatState::Track => self.repeat = RepeatState::None,
        }
    }
}
