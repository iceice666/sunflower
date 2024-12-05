use crate::daemon::Daemon;
use crate::protocol::{
    PlayerRequest, PlayerStateRequest, ProviderRequest, QueueRequest, ResponseKind,
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
            return ResponseKind::Err(e);
        };
    };

    ($context: ident, $remain:ident) => {
        let Ok($remain) = $context else {
            let e = $context.unwrap_err().to_string();
            error!("{}", e);
            return ResponseKind::Err(e);
        };
    };
}

impl Handler<PlayerRequest> for Daemon {
    type Output = ResponseKind;

    fn handle(&self, msg: PlayerRequest) -> ResponseKind {
        match msg {
            PlayerRequest::Play => {
                self.player.play();
                ResponseKind::Ok(None)
            }
            PlayerRequest::Stop => {
                self.player.stop();
                self.state.lock().set_playing(false);
                ResponseKind::Ok(None)
            }
            PlayerRequest::Next => {
                self.player.stop();
                ResponseKind::Ok(None)
            }
            PlayerRequest::Prev => {
                let mut state = self.state.lock();
                state.set_reversed(true);
                self.player.stop();
                ResponseKind::Ok(None)
            }
            PlayerRequest::Pause => {
                self.player.pause();
                ResponseKind::Ok(None)
            }
            PlayerRequest::GetVolume => {
                let vol = self.player.get_volume();
                ResponseKind::Volume(vol)
            }
            PlayerRequest::SetVolume(vol) => {
                self.player.set_volume(vol);
                ResponseKind::Ok(None)
            }
            PlayerRequest::GetPos => {
                let pos = self.player.get_pos();
                ResponseKind::Position(pos)
            }
            PlayerRequest::GetTotalDuration => {
                let total = self.player.get_duration();
                ResponseKind::Total(total)
            }
            PlayerRequest::JumpTo(pos) => {
                let result = self.player.try_seek(pos);
                return_error!(result);
                ResponseKind::Ok(None)
            }
        }
    }
}

// Events dispatches
impl Handler<PlayerStateRequest> for Daemon {
    type Output = ResponseKind;

    fn handle(&self, msg: PlayerStateRequest) -> Self::Output {
        let mut state = self.state.lock();
        match msg {
            PlayerStateRequest::GetRepeat => {
                let repeat = state.get_repeat();
                ResponseKind::Repeat(repeat)
            }
            PlayerStateRequest::SetRepeat(repeat) => {
                state.set_repeat(repeat);
                ResponseKind::Ok(None)
            }
            PlayerStateRequest::GetShuffle => {
                let shuffled = state.is_shuffled();
                ResponseKind::Shuffled(shuffled)
            }
            PlayerStateRequest::ToggleShuffle => {
                state.toggle_shuffle();
                ResponseKind::Ok(None)
            }
            PlayerStateRequest::GetAllState => ResponseKind::CurrentState {
                volume: self.player.get_volume(),
                position: self.player.get_pos(),
                total: self.player.get_duration(),
                repeat: state.get_repeat(),
                shuffled: state.is_shuffled(),
            },
        }
    }
}

impl Handler<ProviderRequest> for Daemon {
    type Output = ResponseKind;

    fn handle(&self, msg: ProviderRequest) -> Self::Output {
        let mut provider_registry = self.provider_registry.lock();

        match msg {
            ProviderRequest::Register(fields) => {
                provider_registry.create(fields);
                ResponseKind::Ok(None)
            }
            ProviderRequest::Unregister(name) => {
                provider_registry.unregister(&name);
                ResponseKind::Ok(None)
            }
            ProviderRequest::SearchTracks {
                query,
                max_results,
                providers,
            } => {
                let result =
                    provider_registry.search(&query, max_results, |name| providers.contains(name));
                return_error!(result, result);
                ResponseKind::TrackSearchResult(result)
            }
            ProviderRequest::GetRegistered => {
                let registers = provider_registry.all_providers();
                ResponseKind::Registers(registers)
            }
        }
    }
}

impl Handler<QueueRequest> for Daemon {
    type Output = ResponseKind;

    fn handle(&self, msg: QueueRequest) -> Self::Output {
        let mut state = self.state.lock();
        match msg {
            QueueRequest::AddTrack {
                provider_name,
                track_id,
            } => {
                let provider_registry = self.provider_registry.lock();
                let track = provider_registry.get_track(&provider_name, &track_id);
                return_error!(track, track);
                state.add(track);

                ResponseKind::Ok(None)
            }
            QueueRequest::RemoveTrack { idx } => {
                state.remove(idx);
                ResponseKind::Ok(None)
            }
            QueueRequest::ClearQueue => {
                state.clear();
                ResponseKind::Ok(None)
            }
            QueueRequest::GetQueue => {
                let queue = state.get_queue();
                ResponseKind::CurrentQueue(queue)
            }
        }
    }
}
