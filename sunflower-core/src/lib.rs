pub(crate) mod daemon;
pub(crate) mod player;

pub mod protocol;
pub(crate) mod provider;
pub(crate) mod source;
pub(crate) mod utils;

pub use crate::daemon::Daemon;
pub use crate::protocol::{
    PlayerRequest, PlayerStateRequest, ProviderRequest, Request, RequestKind, Response,
    ResponseKind, TrackRequest,
};
pub use crate::provider::ProviderFields;

pub fn init_logger() {
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::TRACE)
        .with_file(true)
        .with_line_number(true)
        .try_init();
}
