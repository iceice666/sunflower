mod grpc_server;

use crate::player::{Player, PlayerState};
use crate::provider::ProviderRegistry;
use crate::source::RawAudioSource;

use parking_lot::Mutex;
use std::sync::Arc;
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

    // internal use
    pub(self) __started_time: std::time::Instant,
}

impl Daemon {
    /// Creates a new instance of the Daemon.
    /// And starts the audio player thread and initializes the playing state.
    pub fn new() -> Self {
        let this = Self {
            player: Player::new(),
            state: Mutex::new(PlayerState::new()),
            provider_registry: Mutex::new(ProviderRegistry::new()),
            event_task_handle: Mutex::new(None),
            shutdown_flag: Arc::new(Mutex::new(false)),
            __started_time: std::time::Instant::now(),
        };

        this.start_player_thread();

        this
    }

    /// This method runs in a dedicated thread and continuously processes audio
    /// through the player's main loop.
    /// It uses the `make_source` method to
    /// create new audio sources as needed.
    #[instrument(skip(self))]
    fn start_player_thread(&self) {
        info!("Starting player thread");
        self.state.lock().set_playing(true);

        let source_maker = || self.make_source();
        self.player.main_loop(source_maker);
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
    fn make_source(&self) -> Option<RawAudioSource> {
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
    pub fn shutdown(&self) {
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

use crate::protocol::proto::player_state::PlaybackState;
impl Daemon {
    pub(crate) fn get_playback_state(&self) -> PlaybackState {
        let state = self.state.lock();

        if self.player.is_stopped() {
            PlaybackState::Stopped
        } else if state.is_playing() {
            PlaybackState::Playing
        } else if self.player.is_paused() {
            PlaybackState::Paused
        } else {
            PlaybackState::PlaybackUnknown
        }
    }
}
