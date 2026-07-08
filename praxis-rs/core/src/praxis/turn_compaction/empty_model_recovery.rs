use std::sync::Arc;

use praxis_protocol::models::ResponseItem;
use praxis_utils_output_truncation::TruncationPolicy;
use tracing::warn;

use crate::compact::InitialContextInjection;
use crate::contextual_user_message::RUNTIME_RECOVERY_FRAGMENT;
use crate::praxis::Session;
use crate::praxis::TurnContext;

use super::auto_compact::run_auto_compact;
use super::token_limit::effective_auto_compact_token_limit;

pub(crate) async fn record_empty_model_recovery(
    sess: &Arc<Session>,
    turn_context: &Arc<TurnContext>,
    message: String,
) {
    // Only compact as part of recovery when the context is actually near the
    // auto-compact limit. An empty completion from a small context is a
    // provider fault, and compacting on it repeatedly (with the same broken
    // provider serving the summarization call) shreds the thread's history.
    let total_usage = sess.get_total_token_usage().await;
    let should_compact = effective_auto_compact_token_limit(sess.as_ref(), turn_context.as_ref())
        .is_some_and(|limit| total_usage >= limit);
    if should_compact {
        if let Err(err) = run_auto_compact(
            sess,
            turn_context,
            InitialContextInjection::BeforeLastUserMessage,
        )
        .await
        {
            warn!(
                turn_id = %turn_context.sub_id,
                error = %err,
                "empty model recovery compact failed; retrying with recovery context only"
            );
        }
    } else {
        warn!(
            turn_id = %turn_context.sub_id,
            total_usage,
            "empty model completion below compact limit; skipping recovery compaction"
        );
    }

    let recovery_item = build_empty_model_recovery_item(sess, message).await;
    sess.record_conversation_items(turn_context, std::slice::from_ref(&recovery_item))
        .await;
}

async fn build_empty_model_recovery_item(sess: &Arc<Session>, message: String) -> ResponseItem {
    let latest_user_message = crate::history_preview::HistoryPreview::for_session(sess.as_ref())
        .await
        .latest_user_message(TruncationPolicy::Tokens(2000));

    let mut body = String::from(
        "Runtime recovery retry: the previous model response completed without assistant text or tool calls.",
    );
    body.push_str("\n\n");
    body.push_str(message.trim());
    body.push_str(
        "\n\nTreat the latest non-contextual user message as the active task. Do not summarize old history. If it lists explicit tool steps, call the first required tool now.",
    );
    if let Some(latest_user_message) = latest_user_message
        && !latest_user_message.trim().is_empty()
    {
        body.push_str("\n\nLatest non-contextual user message excerpt:\n");
        body.push_str(latest_user_message.trim());
    }

    RUNTIME_RECOVERY_FRAGMENT.into_message(RUNTIME_RECOVERY_FRAGMENT.wrap(body))
}
