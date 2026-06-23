use praxis_protocol::protocol::AskForApproval;
use praxis_protocol::protocol::Event;

use crate::config::Config;
use crate::praxis::event_delivery::make_warning_event;

pub(super) fn append(events: &mut Vec<Event>, config: &Config) {
    if config.permissions.approval_policy.value() == AskForApproval::OnFailure {
        events.push(make_warning_event(
            "",
            "`on-failure` approval policy is deprecated and will be removed in a future release. Use `on-request` for interactive approvals or `never` for non-interactive runs.",
        ));
    }
}
