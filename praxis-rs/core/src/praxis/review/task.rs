use std::sync::Arc;

use praxis_protocol::protocol::EventMsg;
use praxis_protocol::protocol::PraxisErrorInfo;
use praxis_protocol::protocol::ReviewRequest;

use crate::config::Config;

use super::super::Session;
use super::super::TurnContext;
use super::turn_context;

pub(in crate::praxis) async fn start_review(
    sess: &Arc<Session>,
    config: &Arc<Config>,
    sub_id: String,
    review_request: ReviewRequest,
) {
    let turn_context = sess.new_default_turn_with_sub_id(sub_id.clone()).await;
    sess.maybe_emit_unknown_model_warning_for_turn(turn_context.as_ref())
        .await;
    sess.refresh_mcp_servers_if_requested(&turn_context).await;
    let resolved_review =
        crate::review_prompts::resolve_review_request(review_request, turn_context.cwd.as_path());
    match resolved_review {
        Ok(resolved) => {
            spawn_review_thread(
                Arc::clone(sess),
                Arc::clone(config),
                turn_context.clone(),
                sub_id,
                resolved,
            )
            .await;
        }
        Err(err) => {
            sess.turn_event_emitter(&turn_context)
                .error(err.to_string(), Some(PraxisErrorInfo::Other))
                .await;
        }
    }
}

async fn spawn_review_thread(
    sess: Arc<Session>,
    config: Arc<Config>,
    parent_turn_context: Arc<TurnContext>,
    sub_id: String,
    resolved: crate::review_prompts::ResolvedReviewRequest,
) {
    let review_request = ReviewRequest {
        target: resolved.target,
        user_facing_hint: Some(resolved.user_facing_hint),
    };
    let review_turn = turn_context::build(
        &sess,
        &config,
        &parent_turn_context,
        sub_id,
        resolved.prompt,
    )
    .await;

    review_turn
        .context
        .turn_metadata_state
        .spawn_git_enrichment_task();
    sess.start_review_task(review_turn.context.clone(), review_turn.input)
        .await;
    sess.send_event(
        &review_turn.context,
        EventMsg::EnteredReviewMode(review_request),
    )
    .await;
}
