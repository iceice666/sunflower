pub(crate) mod _impl;
pub mod error;

#[cfg(test)]
mod tests;
pub mod track;

use error::PlayerResult;

use std::sync::mpsc::{Receiver, Sender};
use std::thread;

#[cfg(feature = "interface")]
mod interface;

#[cfg(not(feature = "interface"))]
pub use crate::_impl::*;

#[cfg(debug_assertions)]
/// A simple function that starts player thread in the background.
/// Only for debug use (like unit test).
pub fn play(
    callback: impl Send + 'static + FnOnce(Sender<_impl::EventRequest>, Receiver<_impl::EventResponse>),
) -> PlayerResult<thread::JoinHandle<()>> {
    let (player, sender, receiver) = _impl::Player::try_new()?;

    let handle = thread::spawn(|| callback(sender, receiver));

    // This block current thread
    player.mainloop();

    Ok(handle)
}
