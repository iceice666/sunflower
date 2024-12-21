pub(crate) mod daemon;
pub(crate) mod player;

pub mod protocol;
pub(crate) mod provider;
pub(crate) mod source;
pub(crate) mod utils;

pub use crate::daemon::Daemon;

pub fn init_logger() {
    #[cfg(debug_assertions)]
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::TRACE)
        .with_file(true)
        .with_line_number(true)
        .try_init();

    #[cfg(not(debug_assertions))]
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_file(false)
        .with_line_number(false)
        .try_init();
}


pub mod version {
    include!(concat!(env!("OUT_DIR"), "/git_hash.rs"));
}