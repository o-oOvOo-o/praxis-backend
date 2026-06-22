use std::sync::Arc;

use praxis_protocol::items::build_hook_prompt_message;
use praxis_protocol::protocol::AskForApproval;
use praxis_protocol::protocol::EventMsg;

use super::super::super::Session;
use super::super::super::TurnContext;

pub(super) enum StopHookLifecycleDecision {
    ContinueTurn,
    CompleteTurn,
    RunAfterAgent,
}

pub(super) async fn run_stop_hook_lifecycle(
    sess: &Arc<Session>,
    turn_context: &Arc<TurnContext>,
    last_agent_message: Option<String>,
    stop_hook_active: &mut bool,
) -> StopHookLifecycleDecision {
    let stop_request =
        build_stop_request(sess, turn_context, last_agent_message, *stop_hook_active).await;
    emit_stop_hook_starts(sess, turn_context, &stop_request).await;
    let stop_outcome = sess.hooks().run_stop(stop_request).await;

    for completed in stop_outcome.hook_events {
        sess.send_event(turn_context, EventMsg::HookCompleted(completed))
            .await;
    }

    if stop_outcome.should_block {
        if let Some(hook_prompt_message) =
            build_hook_prompt_message(&stop_outcome.continuation_fragments)
        {
            sess.record_conversation_items(
                turn_context,
                std::slice::from_ref(&hook_prompt_message),
            )
            .await;
            *stop_hook_active = true;
            return StopHookLifecycleDecision::ContinueTurn;
        }
        sess.turn_event_emitter(turn_context)
            .warning("Stop hook requested continuation without a prompt; ignoring the block.")
            .await;
    }

    if stop_outcome.should_stop {
        StopHookLifecycleDecision::CompleteTurn
    } else {
        StopHookLifecycleDecision::RunAfterAgent
    }
}

async fn build_stop_request(
    sess: &Arc<Session>,
    turn_context: &Arc<TurnContext>,
    last_agent_message: Option<String>,
    stop_hook_active: bool,
) -> praxis_hooks::StopRequest {
    praxis_hooks::StopRequest {
        session_id: sess.conversation_id,
        turn_id: turn_context.sub_id.clone(),
        cwd: turn_context.cwd.to_path_buf(),
        transcript_path: sess.hook_transcript_path().await,
        model: turn_context.model_info.slug.clone(),
        permission_mode: stop_hook_permission_mode(turn_context),
        stop_hook_active,
        last_assistant_message: last_agent_message,
    }
}

async fn emit_stop_hook_starts(
    sess: &Arc<Session>,
    turn_context: &Arc<TurnContext>,
    stop_request: &praxis_hooks::StopRequest,
) {
    for run in sess.hooks().preview_stop(stop_request) {
        sess.send_event(
            turn_context,
            EventMsg::HookStarted(praxis_protocol::protocol::HookStartedEvent {
                turn_id: Some(turn_context.sub_id.clone()),
                run,
            }),
        )
        .await;
    }
}

fn stop_hook_permission_mode(turn_context: &TurnContext) -> String {
    match turn_context.effective_approval_policy() {
        AskForApproval::Never => "bypassPermissions",
        AskForApproval::UnlessTrusted
        | AskForApproval::OnFailure
        | AskForApproval::OnRequest
        | AskForApproval::Granular(_) => "default",
    }
    .to_string()
}
