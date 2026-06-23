mod approval_policy;
mod deprecations;
mod unstable_features;
mod warnings;

use praxis_protocol::protocol::Event;

use crate::config::Config;

pub(super) fn build_post_configured_events(config: &Config) -> Vec<Event> {
    let mut events = Vec::new();
    deprecations::append(&mut events, config);
    warnings::append_startup_warnings(&mut events, config);
    unstable_features::append(&mut events, config);
    approval_policy::append(&mut events, config);
    events
}

pub(super) fn hook_warning_event(warning: String) -> Event {
    warnings::hook_warning_event(warning)
}
