use praxis_hooks::Hooks;
use praxis_protocol::protocol::Event;

use crate::config::Config;
use crate::shell::Shell;

use super::super::super::hooks_bootstrap;
use super::super::super::startup_notices;

pub(super) fn build(
    config: &Config,
    default_shell: &Shell,
    post_session_configured_events: &mut Vec<Event>,
) -> Hooks {
    let hooks = hooks_bootstrap::build(config, default_shell);
    for warning in hooks.startup_warnings() {
        post_session_configured_events.push(startup_notices::hook_warning_event(warning.clone()));
    }
    hooks
}
