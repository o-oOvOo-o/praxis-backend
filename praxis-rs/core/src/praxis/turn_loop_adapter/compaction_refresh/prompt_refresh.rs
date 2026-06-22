use std::sync::Arc;

use praxis_loop::TurnError;

use crate::compact::InitialContextInjection;

use super::super::super::Session;
use super::super::super::TurnContext;
use super::super::super::turn_compaction::run_auto_compact;
use super::super::prompt_bridge;
use super::LoopPromptItems;
use super::PromptRefreshDecision;
use super::error;
use super::policy;

pub(in crate::praxis::turn_loop_adapter) async fn prompt_items_from_session_history(
    session: &Arc<Session>,
    turn_context: &Arc<TurnContext>,
) -> LoopPromptItems {
    prompt_bridge::initial_prompt_items_from_session_history(session, turn_context).await
}

pub(in crate::praxis::turn_loop_adapter) async fn auto_compact_prompt_refresh(
    session: &Arc<Session>,
    turn_context: &Arc<TurnContext>,
    injection: InitialContextInjection,
) -> Result<LoopPromptItems, TurnError> {
    run_auto_compact(session, turn_context, injection)
        .await
        .map_err(error::internal_turn_error)?;
    Ok(prompt_items_from_session_history(session, turn_context).await)
}

pub(in crate::praxis::turn_loop_adapter) async fn auto_compact_prompt_refresh_if_needed(
    session: &Arc<Session>,
    turn_context: &Arc<TurnContext>,
    injection: InitialContextInjection,
) -> Result<PromptRefreshDecision, TurnError> {
    if !policy::auto_compact_needed(session, turn_context).await {
        return Ok(PromptRefreshDecision::Unchanged);
    }
    auto_compact_prompt_refresh(session, turn_context, injection)
        .await
        .map(PromptRefreshDecision::Refreshed)
}
