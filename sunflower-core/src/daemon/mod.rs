mod handler;

use crate::daemon::handler::Handler;
use crate::player::{Player, PlayerState};
use crate::protocol::{Request, RequestKind, Response, ResponseKind};
use crate::provider::ProviderRegistry;
use crate::source::RawAudioSource;

use parking_lot::Mutex;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use tracing::{debug, error, info, instrument, trace, warn};

/// A daemon that manages audio playback and handles client requests.
///
/// The `Daemon` struct coordinates audio playback, state management, and request handling
/// through multiple threads.
#[derive(Debug)]
pub struct Daemon {
    /// The core audio player component
    player: Player,
    /// Thread-safe state management for the player
    state: Mutex<PlayerState>,
    /// Registry for managing audio providers
    provider_registry: Mutex<ProviderRegistry>,
    /// Handles for managing events tasks thread
    event_task_handle: Mutex<Option<tokio::task::JoinHandle<()>>>,
    /// Flag to coordinate shutdown across threads
    shutdown_flag: Arc<Mutex<bool>>,
}

impl Daemon {
    /// Creates a new instance of the Daemon wrapped in an Arc.
    ///
    /// Returns an Arc<Daemon> to allow shared ownership across threads.
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            player: Player::new(),
            state: Mutex::new(PlayerState::new()),
            provider_registry: Mutex::new(ProviderRegistry::new()),
            event_task_handle: Mutex::new(None),
            shutdown_flag: Arc::new(Mutex::new(false)),
        })
    }

    /// Starts the audio player thread and initializes the playing state.
    ///
    /// This method runs in a dedicated thread and continuously processes audio
    /// through the player's main loop.
    /// It uses the `make_source` method to
    /// create new audio sources as needed.
    #[instrument(skip(self))]
    fn start_player_thread(self: Arc<Self>) {
        info!("Starting player thread");
        self.state.lock().set_playing(true);

        let this = self.clone();
        let source_maker = || this.clone().make_source();
        this.player.main_loop(source_maker)
    }

    /// Handles incoming requests and manages response distribution.
    ///
    /// This method runs in a dedicated async task and:
    /// 1. Receives requests from clients
    /// 2. Processes them asynchronously
    /// 3. Sends responses back through the response channel
    ///
    /// # Arguments
    /// * `req_rx` - Receiver for incoming requests
    /// * `res_tx` - Sender for outgoing responses
    #[instrument(skip(self, req_rx, res_tx))]
    async fn start_event_thread(
        self: Arc<Self>,
        mut req_rx: UnboundedReceiver<Request>,
        res_tx: UnboundedSender<Response>,
    ) {
        while let Some(request) = req_rx.recv().await {
            let this = self.clone();
            let tx = res_tx.clone();

            // Spawn a new task for each request to handle them concurrently
            tokio::spawn(async move {
                let id = request.id;
                trace!(?id, "Processing incoming request");

                let kind = match request.kind {
                    RequestKind::AreYouAlive => ResponseKind::ImAlive,
                    RequestKind::Terminate => {
                        this.shutdown();
                        ResponseKind::Ok(None)
                    }
                    RequestKind::Player(r) => this.handle(r),
                    RequestKind::State(r) => this.handle(r),
                    RequestKind::Track(r) => this.handle(r),
                    RequestKind::Provider(r) => this.handle(r),
                };

                if let Err(e) = tx.send(Response {
                    kind,
                    id: id.clone(),
                }) {
                    error!(request_id = ?id, "Failed to send response: {}", e);
                } else {
                    trace!(request_id = ?id, "Successfully processed request");
                }
            });
        }
    }

    /// Initializes and starts the daemon, returning communication channels.
    ///
    /// # Returns
    /// A tuple containing:
    /// * Sender for sending requests to the daemon
    /// * Receiver for receiving responses from the daemon
    #[instrument(skip(self))]
    pub fn start(self: Arc<Self>) -> (UnboundedSender<Request>, UnboundedReceiver<Response>) {
        info!("Initializing daemon startup sequence");
        let (req_tx, req_rx) = mpsc::unbounded_channel();
        let (res_tx, res_rx) = mpsc::unbounded_channel();

        // Start the player thread in a blocking context since it may perform I/O
        let this = self.clone();
        tokio::task::spawn_blocking(move || this.clone().start_player_thread());

        // Start the event handling thread
        let event = tokio::spawn(self.clone().start_event_thread(req_rx, res_tx));
        self.event_task_handle.lock().replace(event);

        (req_tx, res_rx)
    }

    /// Creates a new audio source with retry logic.
    ///
    /// This method implements a robust retry mechanism that:
    /// 1. Attempts to create a new audio source
    /// 2. Handles failures with exponential backoff
    /// 3. Respects the playing state and shutdown signals
    ///
    /// # Returns
    /// Option<RawAudioSource> - The created audio source if successful
    #[instrument(skip(self))]
    fn make_source(self: Arc<Self>) -> Option<RawAudioSource> {
        const MAX_RETRIES: u32 = 5;
        const RETRY_DELAY_SECS: u64 = 5;

        let mut retry_count = 0;

        loop {
            let mut state_guard = self.state.lock();

            state_guard.update_index();

            // Wait for play signal or shutdown
            if !state_guard.is_playing() {
                let signal = state_guard.play_signal.clone();
                let shutdown_flag = &*self.shutdown_flag.clone();

                trace!("Waiting signal...");
                signal.wait_while(&mut state_guard, |state| {
                    let shutdown_flag_guard = shutdown_flag.lock();
                    let should_wait = !(state.is_playing() || *shutdown_flag_guard);
                    drop(shutdown_flag_guard);
                    should_wait
                });
            }

            match state_guard.make_source() {
                Ok(source) => {
                    info!("Successfully created new audio source");
                    return Some(source);
                }
                Err(e) => {
                    error!("Failed to create audio source: {:?}", e);
                    retry_count += 1;

                    if retry_count >= MAX_RETRIES {
                        warn!("Maximum retry attempts ({}) exceeded", MAX_RETRIES);
                        retry_count = 0;
                        state_guard.set_playing(false);
                        drop(state_guard);

                        // Exponential backoff
                        std::thread::sleep(std::time::Duration::from_secs(
                            RETRY_DELAY_SECS * (1 << (retry_count.min(3))),
                        ));
                    }
                }
            }
        }
    }

    /// Initiates a graceful shutdown of the daemon.
    ///
    /// This method:
    /// 1. Set shutdown flag
    /// 2. Stops the player
    /// 3. Abort any running tasks
    pub fn shutdown(self: Arc<Self>) {
        info!("Initiating daemon shutdown sequence");
        *self.shutdown_flag.lock() = true;

        let mut state = self.state.lock();
        state.set_playing(false);

        if let Some(handles) = self.event_task_handle.lock().take() {
            debug!("Aborting the event task");
            handles.abort();
        }

        debug!("Shutting down player");
        self.player.shutdown();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::init_logger;
    use tokio::test;

    #[test]
    async fn test_daemon_start_stop() {
        init_logger();

        let daemon = Daemon::new();
        let (tx, mut rx) = daemon.clone().start();

        // Verify daemon responsiveness
        tx.send(RequestKind::AreYouAlive.into())
            .expect("Failed to send alive check request");

        let response = rx
            .recv()
            .await
            .expect("Failed to receive alive check response");
        assert!(matches!(response.kind, ResponseKind::ImAlive));

        daemon.shutdown();
    }
}
