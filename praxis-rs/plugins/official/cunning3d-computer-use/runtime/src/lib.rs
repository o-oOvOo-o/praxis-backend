#![forbid(unsafe_code)]

//! Host-neutral Praxis computer-use tools over composable Metra channels.

mod executor;
mod parameters;
mod permissions;
mod tools;

pub use executor::ComputerUseExecutor;
pub use parameters::{MAX_DYNAMIC_TOOL_TIMEOUT_MS, parse_dynamic_tool_call};
pub use permissions::{PerCallApproval, authorize_computer_use_action};
pub use praxis_utils_approval_presets::PermissionPreset;
pub use tools::{
    COMPUTER_USE_TOOL_NAMESPACE, COMPUTER_USE_TOOL_PREFIX, ComputerUseDynamicTool,
    INTERACT_TOOL_NAME, OBSERVE_TOOL_NAME, computer_use_dynamic_tool, dynamic_tool_definitions,
    is_computer_use_tool_namespace, recognizes_computer_use_tool,
};
