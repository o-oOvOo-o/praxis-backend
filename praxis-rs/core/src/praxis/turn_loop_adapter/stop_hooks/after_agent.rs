use std::sync::Arc;

use praxis_hooks::HookEvent;
use praxis_hooks::HookEventAfterAgent;
use praxis_hooks::HookPayload;
use praxis_hooks::HookResponse;
use praxis_hooks::HookResult;
use tracing::warn;

use super::super::super::Session;
use super::super::super::TurnContext;
use super::TurnStopHooksDecision;

pub(super) async fn run_after_agent_hooks(
    sess: &Arc<Session>,
    turn_context: &Arc<TurnContext>,
    model_request_input_messages: Vec<String>,
    last_agent_message: Option<String>,
) -> TurnStopHooksDecision {
    let hook_outcomes = sess
        .hooks()
        .dispatch(HookPayload {
            session_id: sess.conversation_id,
            cwd: turn_context.cwd.to_path_buf(),
            client: turn_context.app_gateway_client_name.clone(),
            triggered_at: chrono::Utc::now(),
            hook_event: HookEvent::AfterAgent {
                event: HookEventAfterAgent {
                    thread_id: sess.conversation_id,
                    turn_id: turn_context.sub_id.clone(),
                    input_messages: model_request_input_messages,
                    last_assistant_message: last_agent_message,
                },
            },
        })
        .await;

    if let Some(message) = first_abort_message(turn_context, hook_outcomes) {
        sess.turn_event_emitter(turn_context)
            .error(message, None)
            .await;
        TurnStopHooksDecision::AbortTurn
    } else {
        TurnStopHooksDecision::CompleteTurn
    }
}

fn first_abort_message(
    turn_context: &TurnContext,
    hook_outcomes: Vec<HookResponse>,
) -> Option<String> {
    let mut abort_message = None;
    for hook_outcome in hook_outcomes {
        let hook_name = hook_outcome.hook_name;
        match hook_outcome.result {
            HookResult::Success => {}
            HookResult::FailedContinue(error) => {
                warn!(
                    turn_id = %turn_context.sub_id,
                    hook_name = %hook_name,
                    error = %error,
                    "after_agent hook failed; continuing"
                );
            }
            HookResult::FailedAbort(error) => {
                let message = format!(
                    "after_agent hook '{hook_name}' failed and aborted turn completion: {error}"
                );
                warn!(
                    turn_id = %turn_context.sub_id,
                    hook_name = %hook_name,
                    error = %error,
                    "after_agent hook failed; aborting operation"
                );
                if abort_message.is_none() {
                    abort_message = Some(message);
                }
            }
        }
    }
    abort_message
}
