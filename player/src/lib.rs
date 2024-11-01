pub(crate) mod _impl;
pub mod error;
mod interface;

#[cfg(test)]
mod tests;

#[cfg(feature = "interface")]
pub use interface::*;

#[cfg(feature = "low_level")]
pub use _impl::*;

#[cfg(all(feature = "interface", feature = "low_level"))]
compile_error!("Cannot enable both 'interface' and 'low_level' features at the same time.");

use error::PlayerResult;

use std::sync::mpsc::{Receiver, Sender};
use std::thread::JoinHandle;
use std::{collections::HashMap, fmt::Debug, thread};

pub type TrackInfo = HashMap<String, String>;
type TrackSourceType<T> = Box<dyn rodio::Source<Item = T> + Send + Sync>;

pub enum TrackSource {
    F32(TrackSourceType<f32>),
    I16(TrackSourceType<i16>),
    U16(TrackSourceType<u16>),
}

pub trait Track: Send + Sync {
    fn info(&self) -> PlayerResult<TrackInfo>;

    fn build_source(&self) -> PlayerResult<TrackSource>;

    fn get_unique_id(&self) -> String;
}

pub type TrackObject = Box<dyn Track>;

impl Debug for TrackObject {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "TrackObject({})", self.get_unique_id())
    }
}

#[cfg(debug_assertions)]
/// A simple function that starts player thread in the background.
/// Only for debug use (like unit test).
pub fn play(
    callback: impl Send + 'static + FnOnce(Sender<_impl::EventRequest>, Receiver<_impl::EventResponse>),
) -> PlayerResult<JoinHandle<()>> {
    let (player, sender, receiver) = _impl::Player::try_new()?;

    let handle = thread::spawn(|| callback(sender, receiver));

    // This block current thread
    player.mainloop();

    Ok(handle)
}
