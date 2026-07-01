//! Shared Sunflower domain and service layer.
//!
//! This crate is intentionally transport- and database-neutral. Flutter calls it
//! through `sunflower-bridge`; the Rust server calls the same services through
//! `sunflower-server`. Storage details live behind repository traits.

pub mod models;
pub mod queue;
pub mod recommendation;
pub mod repository;
pub mod wire;

pub use models::*;
pub use queue::*;
pub use recommendation::*;
pub use repository::*;
pub use wire::*;
