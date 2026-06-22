use std::sync::Arc;

use crate::compact::InitialContextInjection;
use crate::compact::run_inline_auto_compact_task;
use crate::compact::should_use_remote_compact_task;
use crate::compact_remote::run_inline_remote_auto_compact_task;
use crate::error::Result as PraxisResult;
use crate::praxis::Session;
use crate::praxis::TurnContext;

use super::token_limit::effective_auto_compact_token_limit;

pub(in crate::praxis) async fn run_before_model_request_compact(
    sess: &Arc<Session>,
    turn_context: &Arc<TurnContext>,
) -> PraxisResult<bool> {
    let total_usage_tokens_before_compaction = sess.get_total_token_usage().await;
    let compacted_for_previous_model = maybe_run_previous_model_inline_compact(
        sess,
        turn_context,
        total_usage_tokens_before_compaction,
    )
    .await?;
    let total_usage_tokens = sess.get_total_token_usage().await;
    let auto_compact_limit =
        effective_auto_compact_token_limit(sess.as_ref(), turn_context.as_ref())
            .unwrap_or(i64::MAX);
    if total_usage_tokens >= auto_compact_limit {
        run_auto_compact(sess, turn_context, InitialContextInjection::DoNotInject).await?;
        return Ok(true);
    }
    Ok(compacted_for_previous_model)
}

async fn maybe_run_previous_model_inline_compact(
    sess: &Arc<Session>,
    turn_context: &Arc<TurnContext>,
    total_usage_tokens: i64,
) -> PraxisResult<bool> {
    let Some(previous_turn_settings) = sess.previous_turn_settings().await else {
        return Ok(false);
    };
    let previous_model_turn_context = Arc::new(
        turn_context
            .with_model(previous_turn_settings.model, &sess.services.models_manager)
            .await,
    );

    let Some(old_context_window) = previous_model_turn_context.model_context_window() else {
        return Ok(false);
    };
    let Some(new_context_window) = turn_context.model_context_window() else {
        return Ok(false);
    };
    let new_auto_compact_limit =
        effective_auto_compact_token_limit(sess.as_ref(), turn_context.as_ref())
            .unwrap_or(i64::MAX);
    let should_run = total_usage_tokens > new_auto_compact_limit
        && previous_model_turn_context.model_info.slug != turn_context.model_info.slug
        && old_context_window > new_context_window;
    if should_run {
        run_auto_compact(
            sess,
            &previous_model_turn_context,
            InitialContextInjection::DoNotInject,
        )
        .await?;
        return Ok(true);
    }
    Ok(false)
}

pub(in crate::praxis) async fn run_auto_compact(
    sess: &Arc<Session>,
    turn_context: &Arc<TurnContext>,
    initial_context_injection: InitialContextInjection,
) -> PraxisResult<()> {
    if should_use_remote_compact_task(sess.as_ref(), turn_context.as_ref()) {
        run_inline_remote_auto_compact_task(
            Arc::clone(sess),
            Arc::clone(turn_context),
            initial_context_injection,
        )
        .await?;
    } else {
        run_inline_auto_compact_task(
            Arc::clone(sess),
            Arc::clone(turn_context),
            initial_context_injection,
        )
        .await?;
    }
    Ok(())
}
