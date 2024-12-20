use crate::source::error::SourceResult;
use crate::source::{RawAudioSource, SourceKinds, SourceTrait};
use parking_lot::Condvar;
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::Arc;

#[derive(Debug, PartialEq, Deserialize, Serialize, Copy, Clone)]
pub enum Repeat {
    None,
    Track,
    Queue,
}

#[derive(Debug)]
pub struct PlayerState {
    queue: VecDeque<SourceKinds>,
    repeat: Repeat,

    current_index: usize,

    playing: bool,
    shuffled: bool,
    reversed: bool,

    pub play_signal: Arc<Condvar>,
}

impl Default for PlayerState {
    fn default() -> Self {
        Self::new()
    }
}

impl PlayerState {
    pub fn new() -> Self {
        Self {
            queue: VecDeque::new(),
            repeat: Repeat::None,
            current_index: 0,
            playing: false,
            shuffled: false,
            reversed: false,
            play_signal: Arc::new(Condvar::new()),
        }
    }

    #[inline]
    pub fn add(&mut self, kind: SourceKinds) {
        self.queue.push_back(kind);
    }

    #[inline]
    pub fn remove(&mut self, index: usize) -> Option<SourceKinds> {
        self.queue.remove(index)
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.queue.len()
    }

    #[inline]
    pub fn clear(&mut self) {
        self.queue.clear();
    }

    #[inline]
    pub fn get_queue(&self) -> Vec<String> {
        self.queue.iter().map(|s| s.display_title()).collect()
    }

    #[inline]
    pub fn is_playing(&self) -> bool {
        self.playing && !self.queue.is_empty()
    }

    #[inline]
    pub fn set_playing(&mut self, playing: bool) {
        self.playing = playing;
        self.play_signal.notify_all();
    }

    #[inline]
    pub fn is_shuffled(&self) -> bool {
        self.shuffled
    }

    #[inline]
    pub fn set_shuffled(&mut self, shuffled: bool) {
        self.shuffled = shuffled;
    }

    #[inline]
    pub fn toggle_shuffle(&mut self) {
        self.shuffled = !self.shuffled;
    }

    #[inline]
    pub fn is_reversed(&self) -> bool {
        self.reversed
    }

    #[inline]
    pub fn set_reversed(&mut self, reversed: bool) {
        self.reversed = reversed;
    }

    #[inline]
    pub fn get_repeat(&self) -> Repeat {
        self.repeat
    }

    #[inline]
    pub fn set_repeat(&mut self, repeat: Repeat) {
        self.repeat = repeat;
    }

    #[inline]
    pub fn get_current_index(&self) -> usize {
        self.current_index
    }

    #[inline]
    pub fn get_current_track(&self) -> &SourceKinds {
        self.queue.get(self.current_index).unwrap()
    }

    pub fn update_index(&mut self) {
        match (self.repeat, self.reversed, self.shuffled) {
            (Repeat::Track, _, true) => {
                self.current_index = rand::thread_rng().gen_range(0..self.queue.len());
            }
            (Repeat::Track, _, _) => {}
            (Repeat::None, true, _) => {
                if self.current_index == 0 {
                    self.playing = false;
                } else {
                    self.current_index -= 1;
                }
            }
            (Repeat::None, false, _) => {
                if self.current_index + 1 >= self.queue.len() {
                    self.playing = false;
                    self.current_index = self.queue.len();
                } else {
                    self.current_index += 1;
                }
            }
            (Repeat::Queue, true, _) => {
                self.current_index = if self.current_index == 0 {
                    self.queue.len() - 1
                } else {
                    self.current_index - 1
                };
            }
            (Repeat::Queue, false, _) => {
                self.current_index = (self.current_index + 1) % self.queue.len();
            }
        }
    }

    pub fn make_source(&mut self) -> SourceResult<RawAudioSource> {
        let track = self.queue.get(self.current_index).unwrap();
        track.build_source()
    }
}
