use praxis_protocol::protocol::EventMsg;
use praxis_protocol::protocol::TurnAbortReason;
use praxis_protocol::protocol::TurnAbortedEvent;
use tracing::warn;

use crate::praxis::Session;
use crate::praxis::TurnContext;

pub(super) async fn emit_turn_aborted(
    session: &Session,
    turn_context: &TurnContext,
    reason: TurnAbortReason,
) {
    session
        .send_event(
            turn_context,
            EventMsg::TurnAborted(TurnAbortedEvent {
                turn_id: Some(turn_context.sub_id.clone()),
                reason,
            }),
        )
        .await;
}

pub(super) async fn complete_aborted_runtime_command(session: &Session) {
    if let Err(err) = session
        .services
        .agent_os
        .complete_active_runtime_command_for_thread(
            session.conversation_id,
            /*succeeded*/ false,
            "turn_aborted",
        )
        .await
    {
        warn!("failed to fail AgentOS runtime command for aborted turn: {err}");
    }
}
