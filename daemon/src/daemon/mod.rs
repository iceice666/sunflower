use crate::player::{Player, PlayerState};
use crate::protocol::{EventError, PlayerRequest, PlayerStateRequest, Request, Response};
use crate::source::RawAudioSource;
use parking_lot::Mutex;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use tracing::error;

pub struct Daemon {
    player: Player,
    state: Mutex<PlayerState>,
}

impl Daemon {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            player: Player::new(),
            state: Mutex::new(PlayerState::new()),
        })
    }

    fn set_playing(&self, playing: bool) {
        let mut state = self.state.lock();
        state.set_playing(playing);
    }

    async fn start_player_thread(self: Arc<Self>) {
        self.state.lock().set_playing(true);

        let daemon = self.clone();
        let cb = move || {
            let source_maker = || daemon.clone().make_source();
            daemon.player.main_loop(source_maker)
        };

        tokio::task::spawn_blocking(cb);
    }

    async fn start_event_thread(
        self: Arc<Self>,
    ) -> (UnboundedSender<Request>, UnboundedReceiver<Response>) {
        let (req_tx, mut req_rx) = mpsc::unbounded_channel();
        let (res_tx, res_rx) = mpsc::unbounded_channel();

        tokio::task::spawn(async move {
            loop {
                let request = req_rx.recv().await.expect("Remote disconnected");
                let this = self.clone();
                let tx = res_tx.clone();
                tokio::spawn(async move { this.handle_events(tx, request).await });
            }
        });

        (req_tx, res_rx)
    }

    pub async fn main_loop(
        self: Arc<Self>,
    ) -> (UnboundedSender<Request>, UnboundedReceiver<Response>) {
        self.clone().start_player_thread().await;
        self.start_event_thread().await
    }

    fn make_source(self: Arc<Self>) -> RawAudioSource {
        const MAX_RETRIES: u32 = 5;

        let mut retry_count = 0;

        loop {
            let mut state_guard = self.state.lock();

            state_guard.update_index();

            while !state_guard.is_playing() {
                let signal = state_guard.play_signal.clone();
                signal.wait_while(&mut state_guard, |state| !state.is_playing());
            }

            match state_guard.make_source() {
                Ok(source) => return source,
                Err(e) => {
                    error!("Failed to make source: {:?}", e);

                    retry_count += 1;
                    if retry_count >= MAX_RETRIES {
                        // Exceed max retries counts
                        retry_count = 0; // Reset counts
                        state_guard.set_playing(false); // Set to not playing
                        drop(state_guard); // Drop lock
                    }
                }
            }
        }
    }
}

// Events dispatches
impl Daemon {
    async fn handle_events(self: Arc<Self>, tx: UnboundedSender<Response>, request: Request) {
        let response = self.dispatch_events(request);
        tx.send(response).expect("Remote disconnected");
    }

    fn dispatch_events(&self, request: Request) -> Response {
        match request {
            Request::Player(req) => self.handle_player_events(req),
            Request::State(req) => self.handle_state_events(req),
        }
    }

    fn handle_player_events(&self, request: PlayerRequest) -> Response {
        match request {
            PlayerRequest::Play => {
                self.player.play();
                Response::Ok(None)
            }
            PlayerRequest::Stop => {
                self.player.stop();
                self.set_playing(false);
                Response::Ok(None)
            }
            PlayerRequest::Next => {
                self.player.stop();
                Response::Ok(None)
            }
            PlayerRequest::Prev => {
                let mut state_guard = self.state.lock();
                state_guard.set_reversed(true);
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
            PlayerRequest::JumpTo(pos) => match self.player.try_seek(pos) {
                Ok(()) => Response::Ok(None),
                Err(e) => Response::Err(EventError::from(e).to_string()),
            },
        }
    }

    fn handle_state_events(&self, request: PlayerStateRequest) -> Response {
        match request {
            PlayerStateRequest::GetRepeat => {
                let state_guard = self.state.lock();
                let repeat = state_guard.get_repeat();
                Response::Repeat(repeat)
            }
            PlayerStateRequest::SetRepeat(repeat) => {
                let mut state_guard = self.state.lock();
                state_guard.set_repeat(repeat);
                Response::Ok(None)
            }
            PlayerStateRequest::GetShuffle => {
                let state_guard = self.state.lock();
                let shuffled = state_guard.is_shuffled();
                Response::Shuffled(shuffled)
            }
            PlayerStateRequest::ToggleShuffle => {
                let mut state_guard = self.state.lock();
                state_guard.toggle_shuffle();
                Response::Ok(None)
            }
        }
    }
}
