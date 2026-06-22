use std::sync::Arc;

use praxis_protocol::protocol::EventMsg;
use praxis_protocol::protocol::TurnCompleteEvent;
use tracing::warn;

use crate::goals::GoalRuntimeEvent;
use crate::praxis::Session;
use crate::praxis::TurnContext;

pub(super) async fn emit_turn_complete(
    session: &Arc<Session>,
    turn_context: &Arc<TurnContext>,
    last_agent_message: Option<String>,
    turn_completed: bool,
) {
    if let Err(err) = session
        .goal_runtime_apply(GoalRuntimeEvent::TurnFinished {
            turn_context: turn_context.as_ref(),
            turn_completed,
        })
        .await
    {
        warn!("failed to apply goal turn-finished runtime event: {err}");
    }

    session
        .send_event(
            turn_context.as_ref(),
            EventMsg::TurnComplete(TurnCompleteEvent {
                turn_id: turn_context.sub_id.clone(),
                last_agent_message,
            }),
        )
        .await;

    if let Err(err) = session
        .services
        .agent_os
        .complete_active_runtime_command_for_thread(
            session.conversation_id,
            turn_completed,
            if turn_completed {
                "turn_finished"
            } else {
                "turn_model_error"
            },
        )
        .await
    {
        warn!("failed to complete AgentOS runtime command for finished turn: {err}");
    }
}
