use rodio::{cpal::FromSample, OutputStream, OutputStreamHandle, Sample, Sink, Source};

pub struct Player {
    sink: Sink,
    __stream_handle: OutputStreamHandle,
}

impl Player {
    pub fn new() -> Self {
        let (_stream, __stream_handle) = OutputStream::try_default().unwrap();
        let sink = Sink::try_new(&__stream_handle).unwrap();

        Self {
            sink,
            __stream_handle,
        }
    }

    /// Start the main loop.  
    /// This will block current thread
    pub fn main_loop<S>(&self, mut callback: impl FnMut() -> S)
    where
        S: Source + Send + 'static,
        f32: FromSample<S::Item>,
        S::Item: Sample + Send,
    {
        let source = callback();
        self.sink.append(source);
        self.sink.sleep_until_end();
    }

    #[inline]
    pub fn set_volume(&self, vol: f32) {
        self.sink.set_volume(vol);
    }

    #[inline]
    pub fn get_volume(&self) -> f32 {
        self.sink.volume()
    }

    #[inline]
    pub fn pause(&self) {
        self.sink.pause();
    }

    #[inline]
    pub fn play(&self) {
        self.sink.play();
    }

    #[inline]
    pub fn stop(&self) {
        self.sink.stop();
    }

    #[inline]
    pub fn get_pos(&self) {
        self.sink.get_pos();
    }

    #[inline]
    pub fn is_paused(&self) {
        self.sink.is_paused();
    }

    #[inline]
    pub fn is_playing(&self) -> bool {
        self.sink.empty()
    }
}
