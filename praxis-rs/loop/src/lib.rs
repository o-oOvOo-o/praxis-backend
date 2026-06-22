//! Generic turn orchestration primitives for Praxis.

pub mod compose;
pub mod context;
pub mod decisions;
pub mod guard;
pub mod hooks;
pub mod ids;
pub mod model;
pub mod noop;
pub mod outcome;
mod prompt;
mod round;
pub mod services;
pub mod state;
mod stream;
mod stream_tools;
pub mod tool;
mod turn_finish;
mod turn_items;
mod turn_loop;
mod turn_start;

pub use context::TurnContext;
pub use context::TurnInput;
pub use guard::LoopGuard;
pub use guard::ToolCallAdmission;
pub use guard::ToolCallLimit;
pub use hooks::TurnHooks;
pub use noop::NoopHooks;
pub use outcome::TurnCompletionMessage;
pub use outcome::TurnError;
pub use outcome::TurnErrorKind;
pub use outcome::TurnResult;
pub use services::TurnServices;
pub use state::TurnState;
pub use turn_loop::run_turn;
