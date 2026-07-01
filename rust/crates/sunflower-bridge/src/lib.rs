//! Flutter bridge surface for the shared Rust core.
//!
//! The generated Dart bindings should target functions in this crate. The
//! player remains in Dart/just_audio; all queue, recommendation, and local
//! persistence decisions flow through Rust.

pub mod api;
mod frb_generated;
