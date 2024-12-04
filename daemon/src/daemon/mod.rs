mod handler;
use crate::daemon::handler::Handler;
use crate::player::{Player, PlayerState};
use crate::protocol::{Request, RequestKind, Response, ResponseKind};
use crate::source::RawAudioSource;

use crate::provider::ProviderRegistry;
use parking_lot::Mutex;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use tracing::error;

pub struct Daemon {
    player: Player,
    state: Mutex<PlayerState>,
    provider_registry: Mutex<ProviderRegistry>,
}

impl Daemon {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            player: Player::new(),
            state: Mutex::new(PlayerState::new()),
            provider_registry: Mutex::new(ProviderRegistry::new()),
        })
    }

    fn start_player_thread(self: Arc<Self>) {
        self.state.lock().set_playing(true);

        let this = self.clone();

        let source_maker = || this.clone().make_source();
        this.player.main_loop(source_maker)
    }

    async fn start_event_thread(
        self: Arc<Self>,
        mut req_rx: UnboundedReceiver<Request>,
        res_tx: UnboundedSender<Response>,
    ) {
        loop {
            let request = req_rx.recv().await.expect("Remote disconnected");
            let this = self.clone();
            let tx = res_tx.clone();

            tokio::spawn(async move {
                let id = request.id;
                let kind = match request.kind {
                    RequestKind::Player(r) => this.handle(r),
                    RequestKind::State(r) => this.handle(r),
                    RequestKind::Track(r) => this.handle(r),
                    RequestKind::Provider(r) => this.handle(r),
                };
                let response = Response { kind, id };
                tx.send(response).expect("Remote disconnected");
            });
        }
    }

    pub async fn start(self: Arc<Self>) -> (UnboundedSender<Request>, UnboundedReceiver<Response>) {
        let (req_tx, req_rx) = mpsc::unbounded_channel();
        let (res_tx, res_rx) = mpsc::unbounded_channel();

        let this = self.clone();
        tokio::task::spawn_blocking(move || this.clone().start_player_thread());
        tokio::spawn(self.start_event_thread(req_rx, res_tx));

        (req_tx, res_rx)
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
