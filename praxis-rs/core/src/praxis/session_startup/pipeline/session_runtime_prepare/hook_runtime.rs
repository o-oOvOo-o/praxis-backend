use praxis_capability_runtime::CapabilityScope;
use praxis_protocol::protocol::Event;

use crate::capabilities::HookCapability;
use crate::config::Config;
use crate::shell::Shell;

use super::super::super::hooks_bootstrap;
use super::super::super::startup_notices;

pub(super) fn build(
    config: &Config,
    default_shell: &Shell,
    capability_scope: &CapabilityScope,
    post_session_configured_events: &mut Vec<Event>,
) -> anyhow::Result<HookCapability> {
    let hooks = hooks_bootstrap::build(config, default_shell);
    for warning in hooks.startup_warnings() {
        post_session_configured_events.push(startup_notices::hook_warning_event(warning.clone()));
    }
    crate::capabilities::publish_hooks(capability_scope, hooks)
}
