#[cfg(test)]
use crate::models_manager::collaboration_mode_presets::CollaborationModesConfig;

#[cfg(test)]
use crate::exec::StreamOutput;

mod rollout_reconstruction;
#[cfg(test)]
mod rollout_reconstruction_tests;
mod session_inbox;
mod session_io;
mod turn_interactions;

#[cfg(test)]
use crate::tasks::ReviewTask;
#[cfg(test)]
use praxis_protocol::protocol::InitialHistory;

pub(crate) const INITIAL_SUBMIT_ID: &str = "";

mod agent_task_loop;
mod agent_turn_loop;
mod event_delivery;
mod event_text_projection;
mod facade;
mod history_context;
mod main_agent_loop;
mod mcp_runtime;
mod memory_commands;
pub(crate) mod model_request;
mod permission_ledger;
mod review;
mod session_configuration;
mod session_context_types;
mod session_core;
mod session_handle;
mod session_shutdown;
mod session_startup;
mod skills_commands;
mod steer_input_error;
mod submission_history;
mod thread_lifecycle;
mod turn_compaction;
mod turn_context;
mod turn_loop_adapter;
mod turn_time_context;
mod turn_tool_config;

pub(crate) use agent_task_loop::agent_task_loop;
pub use facade::Praxis;
pub(in crate::praxis) use permission_ledger::PermissionLedger;
use review::errors_to_info;
use review::skills_to_info;
pub(crate) use session_configuration::PreviousTurnSettings;
pub(crate) use session_configuration::SessionConfiguration;
pub(crate) use session_configuration::SessionSettingsUpdate;
pub(crate) use session_context_types::AutoSummaryModelContext;
pub(crate) use session_context_types::AutoTitleModelContext;
pub(crate) use session_context_types::EffectivePermissions;
pub(crate) use session_context_types::LiveEffectivePermissions;
pub(crate) use session_context_types::TurnSkillsContext;
pub(in crate::praxis) use session_context_types::thread_permissions_from_session_configuration;
pub(crate) use session_handle::Session;
pub use steer_input_error::SteerInputError;
pub(crate) use thread_lifecycle::PraxisSpawnArgs;
pub use thread_lifecycle::PraxisSpawnOk;
pub(crate) use thread_lifecycle::SUBMISSION_CHANNEL_CAPACITY;
pub(crate) use thread_lifecycle::SessionLoopTermination;
#[cfg(test)]
pub(crate) use thread_lifecycle::completed_session_loop_termination;
#[cfg(test)]
pub(crate) use thread_lifecycle::session_loop_termination_from_handle;
pub(crate) use turn_context::TurnContext;

use turn_time_context::local_time_context;
use turn_tool_config::multi_agent_mode_for_turn_model;
use turn_tool_config::tool_capabilities_for_turn_model;
use turn_tool_config::tool_wire_profile_for_wire_api;

use event_text_projection::realtime_text_for_event;
#[cfg(test)]
pub(crate) use tests::make_session_and_context;
#[cfg(test)]
pub(crate) use tests::make_session_and_context_with_dynamic_tools_and_rx;
#[cfg(test)]
pub(crate) use tests::make_session_and_context_with_rx;
#[cfg(test)]
pub(crate) use tests::make_session_configuration_for_tests;

#[cfg(test)]
#[path = "praxis_tests.rs"]
mod tests;
