mod command_intent;
mod command_matchers;
mod command_surface;
mod intent_properties;
mod output_summary;
mod profiles;
mod resource_capacity;
mod resource_contract;
mod runtime_mapping;
mod session_scope;
mod tool_intent;

pub(crate) use command_intent::classify_command;
pub(super) use command_surface::denylist_surface;
pub(super) use intent_properties::requires_compile;
pub(super) use intent_properties::requires_cpu_heavy;
pub(super) use intent_properties::requires_dirty_audit;
pub(super) use intent_properties::requires_write;
pub(super) use output_summary::summarize_output;
pub(super) use profiles::builtin_profiles;
pub(super) use resource_capacity::capacity_for_requirement;
#[cfg(test)]
pub(super) use resource_contract::task_resource_allows;
pub(super) use resource_contract::validate_task_action_contract;
pub(super) use runtime_mapping::artifact_type_for_intent;
pub(super) use runtime_mapping::runtime_kind_for_intent;
pub(crate) use session_scope::{
    coordination_scope_for_session_source, profile_for_rank, rank_for_session_source,
};
pub(super) use tool_intent::classify_mutating_tool;
