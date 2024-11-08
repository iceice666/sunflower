pub(crate) mod _impl;
pub mod error;

#[cfg(test)]
mod tests;
pub mod track;

use error::PlayerResult;

use std::sync::mpsc::{Receiver, Sender};
use std::thread;

pub use crate::player::_impl::*;

#[cfg(debug_assertions)]
/// A simple function that starts player thread in the background.
/// Only for debug use (like unit test).
pub async fn play(
    callback: impl Send + 'static + FnOnce(Sender<PlayerRequest>, Receiver<PlayerResponse>),
) -> PlayerResult<thread::JoinHandle<()>> {
    let (player, sender, receiver) = Player::try_new()?;

    let handle = thread::spawn(|| callback(sender, receiver));

    // This block current thread
    player.main_loop().await;

    Ok(handle)
}
