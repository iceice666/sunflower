use rand::prelude::*;
use rodio::{OutputStream, Sink};
use std::{
    fmt::Debug,
    sync::mpsc::{channel, Receiver, Sender},
};
use tracing::{debug, info, trace, warn};

use crate::{error::PlayerResult, TrackObject, TrackSource};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RepeatState {
    Queue,
    Track,
    None,
}

#[derive(Debug)]
pub enum EventRequest {
    Play,
    Pause,
    Stop,
    Next,
    Prev,
    // Seek(u64),
    GetVolume,
    SetVolume(f32),
    GetRepeat,
    SetRepeat(RepeatState),
    ToggleShuffle,

    NewTrack(TrackObject),
}

#[derive(Debug)]
pub enum EventResponse {
    Volume(f32),
    Repeat(RepeatState),
    Ok,
    Error(String),
}

pub struct Player {
    playlist: Vec<TrackObject>,
    current_track_index: usize,
    is_playing: bool,
    sink: Sink,
    __stream: OutputStream,

    __event_queue_receiver: Receiver<EventRequest>,
    __event_response_sender: Sender<EventResponse>,

    // Flags
    repeat: RepeatState,
    is_shuffle: bool,
    is_playlist_going_backwards: bool,
}

impl Debug for Player {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Player")
            .field("playlist", &self.playlist)
            .field("current_track", &self.current_track_index)
            .field("repeat", &self.repeat)
            .field("is_shuffle", &self.is_shuffle)
            .finish()
    }
}

impl Player {
    pub fn try_new() -> PlayerResult<(Self, Sender<EventRequest>, Receiver<EventResponse>)> {
        let (__stream, stream_handle) = OutputStream::try_default()?;
        let sink = Sink::try_new(&stream_handle)?;
        let (event_queue_tx, event_queue_rx) = channel();
        let (event_response_tx, event_response_rx) = channel();

        let this = Self {
            playlist: Vec::new(),
            current_track_index: 0,
            is_playing: false,
            sink,
            __stream,

            __event_queue_receiver: event_queue_rx,
            __event_response_sender: event_response_tx,

            repeat: RepeatState::None,
            is_shuffle: false,
            is_playlist_going_backwards: false,
        };

        Ok((this, event_queue_tx, event_response_rx))
    }

    pub fn mainloop(&mut self) {
        info!("Starting mainloop");
        loop {
            self.dispatch_request();

            if !self.is_playing || !self.sink.empty() {
                continue;
            }

            self.update_current_track();

            if !self.is_playing {
                continue;
            }

            self.append_source();
        }
    }

    #[inline]
    fn update_current_track(&mut self) {
        let reverse = self.is_playlist_going_backwards;
        let playlist_len = self.playlist.len();

        let idx = self.current_track_index;
        self.current_track_index = match (self.is_shuffle, self.repeat) {
            (true, _) => thread_rng().gen_range(0..playlist_len),
            (_, RepeatState::Track) => self.current_track_index,
            (_, RepeatState::Queue) => {
                let next_idx = idx + if reverse { playlist_len - 1 } else { 1 };
                next_idx % playlist_len
            }
            (_, RepeatState::None) => {
                let next_idx = idx + if reverse { playlist_len - 1 } else { 1 };
                if next_idx >= playlist_len {
                    self.is_playing = false;
                    next_idx % (playlist_len + 1) // remain a new space for the new track
                } else {
                    next_idx
                }
            }
        };

        self.is_playlist_going_backwards = false;
        trace!(
            "Player state: index:{}, is_playing:{}, is_shuffle:{}, repeat:{:?}",
            self.current_track_index,
            self.is_playing,
            self.is_shuffle,
            self.repeat
        )
    }

    fn dispatch_request(&mut self) {
        if let Ok(request) = self.__event_queue_receiver.try_recv() {
            debug!("Received request: {:?}", request);

            match request {
                EventRequest::Play => {
                    self.sink.play();
                    self.is_playing = true;
                }
                EventRequest::Pause => self.sink.pause(),
                EventRequest::Stop => {
                    self.sink.stop();
                    self.is_playing = false;
                }
                EventRequest::Next => self.sink.stop(),
                EventRequest::Prev => {
                    self.is_playlist_going_backwards = true;
                    self.sink.stop();
                }
                EventRequest::GetVolume => {
                    let volume = self.sink.volume();
                    self.__event_response_sender
                        .send(EventResponse::Volume(volume))
                        .unwrap()
                }
                EventRequest::SetVolume(volume) => self.sink.set_volume(volume),
                EventRequest::GetRepeat => self
                    .__event_response_sender
                    .send(EventResponse::Repeat(self.repeat))
                    .unwrap(),
                EventRequest::SetRepeat(repeat) => self.repeat = repeat,
                EventRequest::ToggleShuffle => self.is_shuffle = !self.is_shuffle,
                EventRequest::NewTrack(track) => {
                    self.playlist.push(track);

                    if !self.is_playing {
                        self.is_playing = true;

                        if self.current_track_index == self.playlist.len() - 1 {
                            self.append_source();
                        }
                    }
                }
            }
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

    #[inline]
    fn append_source(&mut self) {
        let track = self.playlist.get(self.current_track_index).unwrap();
        info!(
            "Next track:[{}] {}",
            self.current_track_index,
            track.get_unique_id()
        );

        trace!("Building source");
        let source = match track.build_source() {
            Ok(source) => source,
            Err(e) => {
                let err_msg = format!(
                    "Failed to build source for track {}: {}",
                    track.get_unique_id(),
                    e
                );
                warn!("{}", err_msg);
                self.__event_response_sender
                    .send(EventResponse::Error(err_msg))
                    .unwrap();
                return;
            }
        };

        trace!("Appending source to sink");
        match source {
            TrackSource::F32(source) => self.sink.append(source),
            TrackSource::I16(source) => self.sink.append(source),
            TrackSource::U16(source) => self.sink.append(source),
        }
    }
}
