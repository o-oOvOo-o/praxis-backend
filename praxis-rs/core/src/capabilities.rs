mod hooks;
mod mcp;
mod providers;
mod skills;
mod tools;

use praxis_capability_runtime::CapabilityRuntime;
use praxis_capability_runtime::CapabilityScope;
use praxis_capability_runtime::ScopeGraph;
use praxis_capability_runtime::ScopeId;
use praxis_capability_runtime::ScopeKind;
use praxis_hooks::Hooks;
use praxis_protocol::ThreadId;

pub(crate) use hooks::HookCapability;
pub(crate) use hooks::publish_hooks;
pub use mcp::McpDiscoveryCapability;
pub(crate) use mcp::McpConnectionCapability;
pub(crate) use mcp::ResolvedMcpSnapshotCapability;
pub(crate) use mcp::publish_mcp_connections;
pub(crate) use mcp::publish_mcp_discovery;
pub(crate) use mcp::publish_resolved_mcp_snapshot;
pub use providers::ProviderCapability;
pub(crate) use providers::publish_providers;
pub(crate) use skills::ResolvedSkillsCapability;
pub use skills::SkillsCapability;
pub(crate) use skills::publish_resolved_skills;
pub(crate) use skills::publish_skills;
pub(crate) use tools::ToolCapabilities;
pub(crate) use tools::ToolCapability;
pub(crate) use tools::publish_tools;

pub(crate) fn new_runtime() -> CapabilityRuntime {
    CapabilityRuntime::new(ScopeGraph::single_root(
        ScopeId::process(),
        ScopeKind::Process,
    ))
}

pub(crate) fn open_thread_scope(
    runtime: &CapabilityRuntime,
    conversation_id: ThreadId,
) -> anyhow::Result<CapabilityScope> {
    Ok(runtime.open_child_scope(
        ScopeId::new(format!("thread:{conversation_id}"))?,
        ScopeKind::Thread,
        ScopeId::process(),
    )?)
}

#[cfg(test)]
pub(crate) fn test_hook_capability(
    conversation_id: ThreadId,
    hooks: Hooks,
) -> (CapabilityScope, HookCapability) {
    let runtime = new_runtime();
    let scope = open_thread_scope(&runtime, conversation_id).expect("open test thread scope");
    let hook_capability = publish_hooks(&scope, hooks).expect("publish test hooks");
    (scope, hook_capability)
}

#[cfg(test)]
pub(crate) fn test_provider_capability(
    models_manager: std::sync::Arc<crate::models_manager::manager::ModelsManager>,
) -> ProviderCapability {
    let runtime = new_runtime();
    publish_providers(&runtime, models_manager).expect("publish test Providers capability")
}

#[cfg(test)]
pub(crate) fn replace_test_providers(
    session: &mut crate::praxis::Session,
    models_manager: std::sync::Arc<crate::models_manager::manager::ModelsManager>,
) {
    let runtime = session.services._capability_scope.runtime();
    session.services.models_manager =
        publish_providers(&runtime, models_manager).expect("replace test Providers capability");
}
