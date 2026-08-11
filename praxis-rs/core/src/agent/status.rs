use praxis_protocol::protocol::AgentStatus;
use praxis_protocol::protocol::EventMsg;

/// Derive the next agent status from a single emitted event.
/// Returns `None` when the event does not affect status tracking.
pub(crate) fn agent_status_from_event(msg: &EventMsg) -> Option<AgentStatus> {
    match msg {
        EventMsg::TurnStarted(_) => Some(AgentStatus::Running),
        EventMsg::TurnComplete(ev) => Some(AgentStatus::Completed(ev.last_agent_message.clone())),
        EventMsg::TurnAborted(ev) => match ev.reason {
            praxis_protocol::protocol::TurnAbortReason::Interrupted => {
                Some(AgentStatus::Interrupted)
            }
            _ => Some(AgentStatus::Errored(format!("{:?}", ev.reason))),
        },
        EventMsg::Error(ev) if ev.affects_turn_status() => {
            Some(AgentStatus::Errored(ev.message.clone()))
        }
        EventMsg::Error(_) => None,
        EventMsg::ShutdownComplete => Some(AgentStatus::Shutdown),
        _ => None,
    }
}

/// Apply an event without allowing the terminal turn boundary to erase the
/// error that caused that boundary. `TurnComplete` means that the loop has
/// stopped; it is not by itself proof that the turn succeeded.
pub(crate) fn agent_status_after_event(
    current: &AgentStatus,
    msg: &EventMsg,
) -> Option<AgentStatus> {
    if matches!(msg, EventMsg::TurnComplete(_)) && matches!(current, AgentStatus::Errored(_)) {
        return None;
    }
    agent_status_from_event(msg)
}

pub(crate) fn is_final(status: &AgentStatus) -> bool {
    !matches!(
        status,
        AgentStatus::PendingInit | AgentStatus::Running | AgentStatus::Interrupted
    )
}
