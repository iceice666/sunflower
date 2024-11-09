use rand::prelude::*;
use rodio::{OutputStream, Sink};
use std::{
    fmt::Debug,
    str::FromStr,
    sync::mpsc::{channel, Receiver, Sender},
    time::Duration,
};
pub use sunflower_daemon_proto::{Request as PlayerRequest, Response as PlayerResponse};

use sunflower_daemon_proto::{
    request::Payload as RequestPayload, response::Payload as ResponsePayload, RequestType,
    ResponseType, SearchResults,
};
use tracing::{debug, error, info, trace, warn};

use crate::{
    player::{
        error::{PlayerError, PlayerResult},
        track::{TrackObject, TrackSource},
    },
    provider::sources::{ProviderRegistry, Providers},
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

    provider_registry: ProviderRegistry,

    sink: Sink,
    __stream: OutputStream,

    __event_queue_receiver: Receiver<PlayerRequest>,
    __event_response_sender: Sender<PlayerResponse>,

    // Flags
    is_playing: bool,
    is_shuffle: bool,
    is_queue_going_backwards: bool,
    is_terminated: bool,
    repeat: RepeatState,
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
            provider_registry: ProviderRegistry::new(),

            sink,
            __stream,

            __event_queue_receiver: event_queue_rx,
            __event_response_sender: event_response_tx,

            repeat: RepeatState::None,
            is_playing: false,
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
                        payload: Some(ResponsePayload::Error(err_msg)),
                    };
                    self.send_response(resp);
                    return;
                }
            };

            let response = self
                .dispatch_request(req_type, request.payload)
                .await
                .unwrap_or_else(|e| {
                    let err_msg = format!("Failed to handle request: {}", e);
                    PlayerResponse {
                        r#type: ResponseType::Error.into(),
                        payload: Some(ResponsePayload::Error(err_msg)),
                    }
                });

            self.send_response(response);
        }
    }

    async fn dispatch_request(
        &mut self,
        request_type: RequestType,
        request_payload: Option<RequestPayload>,
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
                let volume = parse_request_data(request_payload)?;
                self.sink.set_volume(volume);
                PlayerResponse::ok(None)
            }
            RequestType::GetRepeat => PlayerResponse::ok(Some(self.repeat.into())),
            RequestType::SetRepeat => {
                let repeat = parse_request_data(request_payload)?;
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
                payload: None,
            },
            RequestType::SecretCode => PlayerResponse {
                r#type: ResponseType::HiImYajyuSenpai.into(),
                payload: Some(ResponsePayload::Data(String::from("232 137 175 227 129 132 228 184 150 227 128 129 230 157 165 227 129 132 227 130 136"))),
            },
            RequestType::GetStatus => PlayerResponse {
                r#type: ResponseType::PlayerStatus.into(),
                payload: Some(ResponsePayload::Data(format!(
                    "Queue: {:?}, Current: {}, Repeat: {:?}, Shuffle: {}",
                    self.queue, self.current_track_index, self.repeat, self.is_shuffle
                ))),
            },
            RequestType::ClearQueue => {
                self.queue.clear();
                PlayerResponse::ok(None)
            }
            RequestType::RemoveTrack => {
                let index = parse_request_data(request_payload)?;
                self.queue.remove(index);
                PlayerResponse::ok(None)
            }
            RequestType::AddTrack => {
                let RequestPayload::Track(track) = request_payload.ok_or(PlayerError::EmptyData)? else {
                    return Err(PlayerError::InvalidData);
                };

                let track = self.provider_registry.get_track(track.provider, track.id).await?;
                self.add_track(track);

                PlayerResponse::ok(None)
            }
            RequestType::NewProvider => {
                let RequestPayload::ProviderConfig(provider_config) = request_payload.ok_or(PlayerError::EmptyData)? else {
                    return Err(PlayerError::InvalidData);
                };

                let provider = Providers::try_from(provider_config.config)?;
                self.provider_registry.register(provider).await;

                PlayerResponse::ok(None)
            },
            RequestType::RemoveProvider => {
                let RequestPayload::Track(track) = request_payload.ok_or(PlayerError::EmptyData)? else {
                    return Err(PlayerError::InvalidData);
                };
                let provider_name = track.provider;
                self.provider_registry.unregister(provider_name);
                PlayerResponse::ok(None)
            },
            RequestType::ProviderList => {
                let providers = self.provider_registry.providers();
                PlayerResponse::ok(Some(providers.iter().map(|s| s.to_string()).collect::<Vec<String>>().join(" ")))
            },
            RequestType::ProviderSearch => {
                let RequestPayload::TrackSearch(query) = request_payload.ok_or(PlayerError::EmptyData)? else {
                    return Err(PlayerError::InvalidData);
                };
                let result = self.provider_registry.search(query.query, |provider_name| query.providers.contains(&provider_name)).await?;

                PlayerResponse {
                    r#type: ResponseType::SearchResult.into(),
                    payload: Some(ResponsePayload::SearchResults(result.into())),
                }
            },
            
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

fn parse_request_data<T: FromStr>(data: Option<RequestPayload>) -> PlayerResult<T> {
    let RequestPayload::Data(data) = data.ok_or(PlayerError::EmptyData)? else {
        return Err(PlayerError::InvalidData);
    };

    data.parse().map_err(|_| PlayerError::InvalidData)
}
