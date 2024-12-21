use crate::source::SourceTrait;
use crate::Daemon;
use std::collections::HashMap;

use crate::protocol::proto;
use proto::player_service_server::PlayerService;

use crate::protocol::proto::ProviderSearchResults;
use proto::{
    ControlRequest, GetPlayerStateRequest, HealthCheckRequest, HealthCheckResponse, PlayerState,
    ProviderRequest, ProviderResponse, QueueRequest, QueueResponse, RegisterProviderRequest,
    SearchRequest, SearchResponse, SetPlayerStateRequest, UnregisterProviderRequest,
};
use std::time::{Duration, Instant};
use tonic::{Request, Response, Status};

#[tonic::async_trait]
impl PlayerService for Daemon {
    async fn health_check(
        &self,
        request: Request<HealthCheckRequest>,
    ) -> Result<Response<HealthCheckResponse>, Status> {
        let msg = request.into_inner();

        if msg.terminate {
            self.shutdown();
        }

        let now = Instant::now();
        let uptime = now.duration_since(self.__started_time);
        let mut seconds = uptime.as_secs();

        let days = seconds / 86400;
        seconds %= 86400;
        let hours = seconds / 3600;
        seconds %= 3600;
        let minutes = seconds / 60;
        seconds %= 60;

        let uptime = format!("{}:{:02}:{:02}:{:02}", days, hours, minutes, seconds);

        let resp = HealthCheckResponse {
            is_alive: msg.terminate,
            commit_hash: crate::version::GIT_HASH.to_string(),
            uptime,
        };

        Ok(Response::new(resp))
    }

    async fn control(&self, request: Request<ControlRequest>) -> Result<Response<()>, Status> {
        use proto::control_request::{Command, Params};
        use proto::{SeekParams, VolumeParams};

        let msg = request.into_inner();

        let Ok(cmd) = Command::try_from(msg.command) else {
            return Err(Status::invalid_argument("Invalid command"));
        };

        let result: Result<(), Status> = match cmd {
            Command::Unspecified => Ok(()),
            Command::Play => {
                self.player.play();
                Ok(())
            }
            Command::Pause => {
                self.player.pause();
                Ok(())
            }
            Command::Stop => {
                self.player.stop();
                self.state.lock().set_playing(false);
                Ok(())
            }
            Command::Next => {
                self.player.stop();
                Ok(())
            }
            Command::Previous => {
                self.state.lock().set_reversed(true);
                self.player.stop();
                Ok(())
            }
            Command::Seek => {
                if let Some(Params::VolumeParams(VolumeParams { volume })) = msg.params {
                    self.player.set_volume(volume);
                    Ok(())
                } else {
                    Err(Status::invalid_argument("Missing 'volume_params' field"))
                }
            }
            Command::SetVolume => {
                let duration: Result<Duration, Status> =
                    if let Some(Params::SeekParams(SeekParams {
                        position: Some(duration),
                    })) = msg.params
                    {
                        duration
                            .try_into()
                            .map_err(|e: prost_types::DurationError| {
                                Status::invalid_argument(e.to_string())
                            })
                    } else {
                        Err(Status::invalid_argument("Missing 'seek_params' field"))
                    };

                duration.and_then(|dur| {
                    self.player
                        .try_seek(dur)
                        .map_err(|e| Status::internal(e.to_string()))
                })
            }
        };

        result.map(|_| Response::new(()))
    }

    async fn get_player_state(
        &self,
        _: Request<GetPlayerStateRequest>,
    ) -> Result<Response<PlayerState>, Status> {
        use proto::player_state::RepeatMode;
        use proto::Track;
        use std::fmt::Write;

        let mut error_message = String::new();

        // Stage error message then discard the error
        let position = self.player.get_duration().and_then(|dur| {
            dur.try_into()
                .map_err(|e: prost_types::DurationError| writeln!(error_message, "{}", e))
                .ok()
        });

        let duration = Some(self.player.get_pos()).and_then(|dur| {
            dur.try_into()
                .map_err(|e: prost_types::DurationError| writeln!(error_message, "{}", e))
                .ok()
        });

        // If the error message isn't empty, means an error occurs
        if !error_message.is_empty() {
            return Err(Status::invalid_argument(error_message));
        }

        let state = self.state.lock();
        let source = state.get_current_track();

        let track = Track {
            id: source.get_unique_id(),
            source_kind: source.get_source_kind().to_string(),
            title: source.display_title(),
            metadata: source.info().unwrap(),
        };

        let resp = PlayerState {
            playback_state: self.get_playback_state().into(),
            volume: self.player.get_volume(),
            position,
            duration,
            repeat_mode: RepeatMode::from(state.get_repeat()).into(),
            shuffle_enabled: state.is_shuffled(),
            current_track: Some(track),
        };

        Ok(Response::new(resp))
    }

    async fn set_player_state(
        &self,
        request: Request<SetPlayerStateRequest>,
    ) -> Result<Response<()>, Status> {
        use proto::player_state::RepeatMode;

        let msg = request.into_inner();

        if let Some(repeat_mode) = msg.repeat_mode {
            let mode = RepeatMode::try_from(repeat_mode).expect("Invalid RepeatMode");
            self.state.lock().set_repeat(mode.into());
        }

        if let Some(shuffle_enabled) = msg.shuffle_enabled {
            self.state.lock().set_shuffled(shuffle_enabled);
        }

        Ok(Response::new(()))
    }

    async fn manage_queue(
        &self,
        request: Request<QueueRequest>,
    ) -> Result<Response<QueueResponse>, Status> {
        use proto::queue_request::Action;
        use proto::{AddTrackRequest, RemoveTrackRequest};

        let QueueRequest {
            action: Some(action),
        } = request.into_inner()
        else {
            return Err(Status::invalid_argument("Invalid action"));
        };

        let result: Result<QueueResponse, Status> = match action {
            Action::AddTrack(AddTrackRequest {
                provider_id,
                track_id,
            }) => {
                let track = self
                    .provider_registry
                    .lock()
                    .get_track(&provider_id, &track_id);

                let mut state = self.state.lock();

                match track {
                    Ok(track) => {
                        state.add(track);
                        Err(Status::ok(""))
                    }
                    Err(e) => Err(Status::invalid_argument(e.to_string())),
                }
            }
            Action::RemoveTrack(RemoveTrackRequest { index }) => {
                let mut state = self.state.lock();
                state.remove(index as usize);
                Err(Status::ok(""))
            }
            Action::ClearQueue(_) => {
                let mut state = self.state.lock();
                state.clear();
                Err(Status::ok(""))
            }
            Action::GetQueue(_) => {
                let state = self.state.lock();
                let queue = state.get_queue();

                Ok(QueueResponse {
                    tracks: queue,
                    current_index: state.get_current_index() as u32,
                })
            }
        };

        result.map(Response::new)
    }

    async fn manage_provider(
        &self,
        request: Request<ProviderRequest>,
    ) -> Result<Response<ProviderResponse>, Status> {
        use proto::provider_request::Action;

        let mut provider_registry = self.provider_registry.lock();

        let msg = request.into_inner();
        let Some(cmd) = msg.action else {
            return Err(Status::invalid_argument("Invalid action"));
        };

        match cmd {
            Action::Register(RegisterProviderRequest {
                provider: Some(provider),
            }) => match provider.try_into() {
                Ok(kind) => {
                    provider_registry.register(kind);
                    Err(Status::ok(""))
                }
                Err(e) => Err(Status::invalid_argument(e)),
            },
            Action::Unregister(UnregisterProviderRequest { provider_id }) => {
                provider_registry.unregister(&provider_id);
                Err(Status::ok(""))
            }
            Action::GetRegistered(()) => {
                let result = provider_registry.all_providers();

                Ok(ProviderResponse {
                    providers: Vec::from_iter(result),
                })
            }

            _ => Err(Status::invalid_argument("Invalid action")),
        }
        .map(Response::new)
    }

    async fn search_tracks(
        &self,
        request: Request<SearchRequest>,
    ) -> Result<Response<SearchResponse>, Status> {
        let SearchRequest {
            query,
            provider_ids,
            max_results,
        } = request.into_inner();

        let mut provider_registry = self.provider_registry.lock();

        #[rustfmt::skip]
        let result = provider_registry.search(
            &query, max_results.map(|v| v as usize), |p| provider_ids.contains(p)
        );

        match result {
            Ok(tracks) => {
                let mut results = HashMap::new();

                for (k, v) in tracks {
                    results.insert(k, ProviderSearchResults { results: v });
                }

                Ok(Response::new(SearchResponse { results }))
            }
            Err(e) => Err(Status::internal(e.to_string())),
        }
    }
}
