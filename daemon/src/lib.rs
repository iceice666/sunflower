#[cfg(test)]
mod tests;

mod daemon;
pub(crate) mod player;

pub mod protocol;
pub mod provider;
pub mod source;

mod utils;

pub fn init_logger() {
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::TRACE)
        .try_init();
}
