use crate::source::RawTrackSource;
use rodio::source::SeekError;
use rodio::{OutputStream, OutputStreamHandle, Sink};
use single_value_channel::{Receiver, Updater};
use std::time::Duration;

pub struct Player {
    sink: Sink,
    __stream_handle: OutputStreamHandle,
    __current_source_duration: Option<Duration>,
    __current_source_duration_tx: Updater<Option<Duration>>,
    __current_source_duration_rx: Receiver<Option<Duration>>,
}

impl Default for Player {
    fn default() -> Self {
        Self::new()
    }
}

impl Player {
    /// Creates a new instance of the `Player`.
    ///
    /// # Panic
    ///
    /// Panic if the default output stream or the `Sink` cannot be created.
    pub fn new() -> Self {
        let (_stream, __stream_handle) = OutputStream::try_default().unwrap();
        let sink = Sink::try_new(&__stream_handle).unwrap();
        let (rx, tx) = single_value_channel::channel();
        Self {
            sink,
            __stream_handle,
            __current_source_duration: None,
            __current_source_duration_tx: tx,
            __current_source_duration_rx: rx,
        }
    }

    /// Starts the main loop.
    ///
    /// This will block the current thread.
    /// The provided callback is used to get the initial `Source` to be played.
    ///
    /// # Parameters
    /// - `callback`: A callback function that provides a `Source` when called.
    pub fn main_loop(&self, callback: impl FnOnce() -> RawTrackSource) {
        let source = callback();
        let duration = source.total_duration();
        self.__current_source_duration_tx.update(duration).unwrap();
        match source {
            RawTrackSource::I16(src) => self.sink.append(src),
            RawTrackSource::U16(src) => self.sink.append(src),
            RawTrackSource::F32(src) => self.sink.append(src),
        }
        self.sink.sleep_until_end();
    }

    /// Sets the volume of the player.
    ///
    /// # Parameters
    /// - `vol`: The volume level as a float, where `1.0` is the original volume.
    #[inline]
    pub fn set_volume(&self, vol: f32) {
        self.sink.set_volume(vol);
    }

    /// Gets the current volume of the player.
    ///
    /// # Returns
    /// A float representing the volume level.
    #[inline]
    pub fn get_volume(&self) -> f32 {
        self.sink.volume()
    }

    /// Pauses the player.
    #[inline]
    pub fn pause(&self) {
        self.sink.pause();
    }

    /// Resumes playback if the player was paused.
    #[inline]
    pub fn play(&self) {
        self.sink.play();
    }

    /// Stops the player.
    #[inline]
    pub fn stop(&self) {
        self.sink.stop();
    }

    /// Gets the duration of the currently loaded `Source`.
    ///
    /// # Returns
    /// The duration as an `Option<Duration>`.
    /// If the duration is not available, it returns `None`.
    #[inline]
    pub fn get_duration(&mut self) -> Option<Duration> {
        *self.__current_source_duration_rx.latest()
    }

    /// Gets the current playback position.
    ///
    /// # Returns
    /// The current playback position as a `Duration`.
    #[inline]
    pub fn get_pos(&self) -> Duration {
        self.sink.get_pos()
    }

    /// Attempts to seek to a specific position in the current `Source`.
    ///
    /// # Parameters
    /// - `pos`: The position to seek to as a `Duration`.
    ///
    /// # Returns
    /// A `Result` indicating success or failure.
    /// If seeking is not supported, it returns a `SeekError`.
    #[inline]
    pub fn try_seek(&self, pos: Duration) -> Result<(), SeekError> {
        self.sink.try_seek(pos)
    }

    /// Checks if the player is paused.
    ///
    /// # Returns
    /// `true` if the player is paused, otherwise `false`.
    #[inline]
    pub fn is_paused(&self) -> bool {
        self.sink.is_paused()
    }

    /// Checks if the player is playing.
    ///
    /// # Returns
    /// `true` if the player is playing, otherwise `false`.
    #[inline]
    pub fn is_playing(&self) -> bool {
        self.sink.empty()
    }
}
