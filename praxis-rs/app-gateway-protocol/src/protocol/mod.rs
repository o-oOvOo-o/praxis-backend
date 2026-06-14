// Module declarations for the app-gateway protocol namespace.
// Exposes protocol pieces used by `lib.rs` via `pub use protocol::common::*;`.

pub mod api;
pub mod common;
mod serde_helpers;
pub mod thread_history;
pub mod thread_history_policy;
