use praxis_protocol::protocol::Event;

use crate::config::Config;
use crate::praxis::INITIAL_SUBMIT_ID;
use crate::praxis::event_delivery::make_warning_event;

pub(super) fn append_startup_warnings(events: &mut Vec<Event>, config: &Config) {
    for message in &config.startup_warnings {
        events.push(make_warning_event("", message.clone()));
    }
}

pub(super) fn hook_warning_event(warning: String) -> Event {
    make_warning_event(INITIAL_SUBMIT_ID, warning)
}
