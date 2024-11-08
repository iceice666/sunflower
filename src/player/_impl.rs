use rand::prelude::*;
use rodio::{OutputStream, Sink};
use std::{
    fmt::Debug,
    str::FromStr,
    sync::mpsc::{channel, Receiver, Sender},
    time::Duration,
};
pub use sunflower_daemon_proto::{Request as PlayerRequest, Response as PlayerResponse};
use sunflower_daemon_proto::{RequestType, ResponseType};
use tracing::{debug, error, info, trace, warn};

use crate::player::{
    error::{PlayerError, PlayerResult},
    track::{TrackObject, TrackSource},
};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RepeatState {
    Queue,
    Track,
    None,
}

impl FromStr for RepeatState {
    type Err = PlayerError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "queue" => Ok(RepeatState::Queue),
            "track" => Ok(RepeatState::Track),
            "none" => Ok(RepeatState::None),
            _ => Err(PlayerError::InvalidData),
        }
    }
}

impl From<RepeatState> for String {
    fn from(val: RepeatState) -> Self {
        match val {
            RepeatState::Queue => "queue".to_string(),
            RepeatState::Track => "track".to_string(),
            RepeatState::None => "none".to_string(),
        }
    }
}

pub struct Player {
    queue: Vec<TrackObject>,
    current_track_index: usize,
    is_playing: bool,
    sink: Sink,
    __stream: OutputStream,

    __event_queue_receiver: Receiver<PlayerRequest>,
    __event_response_sender: Sender<PlayerResponse>,

    // Flags
    repeat: RepeatState,
    is_shuffle: bool,
    is_queue_going_backwards: bool,
    is_terminated: bool,
}

impl Debug for Player {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Player")
            .field("queue", &self.queue)
            .field("current_track", &self.current_track_index)
            .field("repeat", &self.repeat)
            .field("is_shuffle", &self.is_shuffle)
            .finish()
    }
}

impl Player {
    pub fn try_new() -> PlayerResult<(Self, Sender<PlayerRequest>, Receiver<PlayerResponse>)> {
        let (__stream, stream_handle) = OutputStream::try_default()?;
        let sink = Sink::try_new(&stream_handle)?;
        let (event_queue_tx, event_queue_rx) = channel();
        let (event_response_tx, event_response_rx) = channel();

        let this = Self {
            queue: Vec::new(),
            current_track_index: 0,
            is_playing: false,
            sink,
            __stream,

            __event_queue_receiver: event_queue_rx,
            __event_response_sender: event_response_tx,

            repeat: RepeatState::None,
            is_shuffle: false,
            is_queue_going_backwards: false,
            is_terminated: false,
        };

        Ok((this, event_queue_tx, event_response_rx))
    }

    pub async fn main_loop(mut self) {
        info!("Starting main loop");
        while !self.is_terminated {
            self.handle_request().await;

            // Player is in `play` state and there no more data to play.
            if self.is_playing && self.sink.empty() {
                self.update_current_track();

                // If after update the next track still in `play` state, append the next track.
                if self.is_playing {
                    self.append_source();
                }
            }
        }
    }

    #[inline]
    fn update_current_track(&mut self) {
        let reverse = self.is_queue_going_backwards;
        let queue_len = self.queue.len();

        let idx = self.current_track_index;
        self.current_track_index = match (self.is_shuffle, self.repeat) {
            (true, _) => thread_rng().gen_range(0..queue_len),
            (_, RepeatState::Track) => self.current_track_index,
            (_, RepeatState::Queue) => {
                let next_idx = idx + if reverse { queue_len - 1 } else { 1 };
                next_idx % queue_len
            }
            (_, RepeatState::None) => {
                let next_idx = idx + if reverse { queue_len - 1 } else { 1 };
                if next_idx >= queue_len {
                    self.is_playing = false;
                    next_idx % (queue_len + 1) // remain a new space for the new track
                } else {
                    next_idx
                }
            }
        };

        self.is_queue_going_backwards = false;
        debug!(
            "Player state: index:{}/{}, is_playing:{}, is_shuffle:{}, repeat:{:?}",
            self.current_track_index,
            self.queue.len(),
            self.is_playing,
            self.is_shuffle,
            self.repeat
        )
    }

    #[inline]
    fn send_response(&mut self, response: PlayerResponse) {
        let Err(e) = self.__event_response_sender.send(response) else {
            return;
        };

        error!("Failed to send response: {}", e);
    }

    async fn handle_request(&mut self) {
        if let Ok(request) = self
            .__event_queue_receiver
            .recv_timeout(Duration::from_millis(100))
        // Only block current thread for at the most 100 ms.
        {
            info!("Received request: {:?}", request);

            let req_type = match RequestType::try_from(request.r#type) {
                Ok(req_type) => req_type,
                Err(e) => {
                    let err_msg = format!("Invalid request type: {}", e);
                    let resp = PlayerResponse {
                        r#type: ResponseType::Error.into(),
                        data: Some(err_msg),
                    };
                    self.send_response(resp);
                    return;
                }
            };

            let req_data = request.data;

            let response = self
                .dispatch_request(req_type, req_data)
                .await
                .unwrap_or_else(|e| {
                    let err_msg = format!("Failed to handle request: {}", e);
                    PlayerResponse {
                        r#type: ResponseType::Error.into(),
                        data: Some(err_msg),
                    }
                });

            self.send_response(response);
        }
    }

    async fn dispatch_request(
        &mut self,
        request_type: RequestType,
        request_data: Option<String>,
    ) -> PlayerResult<PlayerResponse> {
        Ok(match request_type {
            RequestType::Play => {
                self.sink.play();
                self.is_playing = true;
                PlayerResponse::ok(None)
            }
            RequestType::Pause => {
                self.sink.pause();
                PlayerResponse::ok(None)
            }
            RequestType::Stop => {
                self.sink.stop();
                self.is_playing = false;
                PlayerResponse::ok(None)
            }
            RequestType::Next => {
                self.sink.stop();
                PlayerResponse::ok(None)
            }
            RequestType::Prev => {
                self.is_queue_going_backwards = true;
                self.current_track_index %= self.queue.len();
                self.sink.stop();
                PlayerResponse::ok(None)
            }
            RequestType::GetVolume => {
                let volume = self.sink.volume();
                PlayerResponse::ok(Some(volume.to_string()))
            }
            RequestType::SetVolume => {
                let volume = parse_request_data(request_data)?;
                self.sink.set_volume(volume);
                PlayerResponse::ok(None)
            }
            RequestType::GetRepeat => PlayerResponse::ok(Some(self.repeat.into())),
            RequestType::SetRepeat => {
                let repeat = parse_request_data(request_data)?;
                self.repeat = repeat;
                PlayerResponse::ok(None)
            }
            RequestType::ToggleShuffle => {
                self.is_shuffle = !self.is_shuffle;
                PlayerResponse::ok(None)
            }
            RequestType::Terminate => {
                info!("Exiting main loop");
                self.is_terminated = true;
                PlayerResponse::ok(None)
            }
            RequestType::CheckAlive => PlayerResponse {
                r#type: ResponseType::ImAlive.into(),
                data: None,
            },
            RequestType::SecretCode => PlayerResponse {
                r#type: ResponseType::HiImYajyuSenpai.into(),
                data: Some(String::from("良い世、来いよ")),
            },
            RequestType::GetStatus => PlayerResponse {
                r#type: ResponseType::PlayerStatus.into(),
                data: Some(format!(
                    "Queue: {:?}, Current: {}, Repeat: {:?}, Shuffle: {}",
                    self.queue, self.current_track_index, self.repeat, self.is_shuffle
                )),
            },
            RequestType::ClearQueue => {
                self.queue.clear();
                PlayerResponse::ok(None)
            }
            RequestType::RemoveTrack => {
                let index = parse_request_data(request_data)?;
                self.queue.remove(index);
                PlayerResponse::ok(None)
            }
            _ => unreachable!("This request should be handled before here"),
        })
    }

    pub fn add_track(&mut self, track: TrackObject) {
        self.queue.push(track);

        if !self.is_playing {
            self.is_playing = true;

            if self.current_track_index == self.queue.len() - 1 {
                self.append_source();
            }
        }
    }

    // #[inline]
    // fn refresh_sink(&mut self) -> PlayerResult {
    //     self.sink.stop();
    //     let (__stream, stream_handle) = OutputStream::try_default()?;
    //     let sink = Sink::try_new(&stream_handle)?;
    //
    //     self.sink = sink;
    //     self.__stream = __stream;
    //
    //     Ok(())
    // }

    #[inline]
    fn append_source(&mut self) {
        let track = self.queue.get(self.current_track_index).unwrap();
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
                    .send(PlayerResponse::err(err_msg))
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

fn parse_request_data<T: FromStr>(data: Option<String>) -> PlayerResult<T> {
    data.ok_or(PlayerError::EmptyData)?
        .parse()
        .map_err(|_| PlayerError::InvalidData)
}
