use std::sync::Arc;

use praxis_protocol::protocol::TokenUsage;

use crate::praxis::Session;
use crate::praxis::TurnContext;

pub(super) async fn run_post_completion_updates(
    session: &Arc<Session>,
    turn_context: &Arc<TurnContext>,
    turn_token_usage: Option<&TokenUsage>,
    last_agent_message_for_title: Option<String>,
    last_agent_message_for_summary: Option<String>,
) {
    crate::thread_cost::persist_turn_cost_estimate(
        session,
        &turn_context.model_info.slug,
        turn_token_usage,
    )
    .await;
    crate::auto_title::maybe_auto_generate_title(session, last_agent_message_for_title).await;
    crate::auto_summary::maybe_auto_generate_summary(session, last_agent_message_for_summary).await;
}
