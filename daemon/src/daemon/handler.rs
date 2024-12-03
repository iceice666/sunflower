use crate::daemon::Daemon;
use crate::protocol::{
    PlayerRequest, PlayerStateRequest, ProviderRequest, Response, TrackRequest,
};
use tracing::error;

pub trait Handler<T> {
    type Output;

    fn handle(&self, msg: T) -> Self::Output;
}

macro_rules! return_error {
    ($context: ident) => {
        if let Err(err) = $context {
            let e = err.to_string();
            error!("{}", e);
            return Response::Err(e);
        };
    };

    ($context: ident, $remain:ident) => {
        let Ok($remain) = $context else {
            let e = $context.unwrap_err().to_string();
            error!("{}", e);
            return Response::Err(e);
        };
    };
}

impl Handler<PlayerRequest> for Daemon {
    type Output = Response;

    fn handle(&self, msg: PlayerRequest) -> Response {
        match msg {
            PlayerRequest::Play => {
                self.player.play();
                Response::Ok(None)
            }
            PlayerRequest::Stop => {
                self.player.stop();
                self.state.lock().set_playing(false);
                Response::Ok(None)
            }
            PlayerRequest::Next => {
                self.player.stop();
                Response::Ok(None)
            }
            PlayerRequest::Prev => {
                let mut state = self.state.lock();
                state.set_reversed(true);
                self.player.stop();
                Response::Ok(None)
            }
            PlayerRequest::Pause => {
                self.player.pause();
                Response::Ok(None)
            }
            PlayerRequest::GetVolume => {
                let vol = self.player.get_volume();
                Response::Volume(vol)
            }
            PlayerRequest::SetVolume(vol) => {
                self.player.set_volume(vol);
                Response::Ok(None)
            }
            PlayerRequest::GetPos => {
                let pos = self.player.get_pos();
                Response::Position(pos)
            }
            PlayerRequest::GetTotalDuration => {
                let total = self.player.get_duration();
                Response::Total(total)
            }
            PlayerRequest::JumpTo(pos) => {
                let result = self.player.try_seek(pos);
                return_error!(result);
                Response::Ok(None)
            }
        }
    }
}

// Events dispatches
impl Handler<PlayerStateRequest> for Daemon {
    type Output = Response;

    fn handle(&self, msg: PlayerStateRequest) -> Self::Output {
        let mut state = self.state.lock();
        match msg {
            PlayerStateRequest::GetRepeat => {
                let repeat = state.get_repeat();
                Response::Repeat(repeat)
            }
            PlayerStateRequest::SetRepeat(repeat) => {
                state.set_repeat(repeat);
                Response::Ok(None)
            }
            PlayerStateRequest::GetShuffle => {
                let shuffled = state.is_shuffled();
                Response::Shuffled(shuffled)
            }
            PlayerStateRequest::ToggleShuffle => {
                state.toggle_shuffle();
                Response::Ok(None)
            }
        }
    }
}

impl Handler<ProviderRequest> for Daemon {
    type Output = Response;

    fn handle(&self, msg: ProviderRequest) -> Self::Output {
        let mut provider_registry = self.provider_registry.lock();

        match msg {
            ProviderRequest::Register(fields) => {
                provider_registry.create(fields);
                Response::Ok(None)
            }
            ProviderRequest::Unregister(name) => {
                provider_registry.unregister(&name);
                Response::Ok(None)
            }
            ProviderRequest::SearchTracks {
                query,
                max_results,
                providers,
            } => {
                let result =
                    provider_registry.search(&query, max_results, |name| providers.contains(name));
                return_error!(result, result);
                Response::TrackSearchResult(result)
            }
        }
    }
}

impl Handler<TrackRequest> for Daemon {
    type Output = Response;

    fn handle(&self, msg: TrackRequest) -> Self::Output {
        let mut state = self.state.lock();
        match msg {
            TrackRequest::AddTrack {
                provider_name,
                track_id,
            } => {
                let provider_registry = self.provider_registry.lock();
                let track = provider_registry.get_track(&provider_name, &track_id);
                return_error!(track, track);
                state.add(track);

                Response::Ok(None)
            }
            TrackRequest::RemoveTrack { idx } => {
                state.remove(idx);
                Response::Ok(None)
            }
        }
    }
}
