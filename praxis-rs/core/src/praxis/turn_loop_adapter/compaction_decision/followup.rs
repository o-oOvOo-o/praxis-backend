use std::sync::Arc;

use praxis_loop::TurnError;

use crate::compact::InitialContextInjection;

use super::super::super::Session;
use super::super::super::TurnContext;
use super::super::compaction_refresh;

type PromptRefreshDecision = compaction_refresh::PromptRefreshDecision;

pub(in crate::praxis::turn_loop_adapter) async fn compact_after_tool_round_if_needed(
    session: &Arc<Session>,
    turn_context: &Arc<TurnContext>,
) -> Result<PromptRefreshDecision, TurnError> {
    refresh_before_last_user_message_if_needed(session, turn_context).await
}

pub(in crate::praxis::turn_loop_adapter) async fn compact_before_followup_after_model_round_if_needed(
    session: &Arc<Session>,
    turn_context: &Arc<TurnContext>,
) -> Result<PromptRefreshDecision, TurnError> {
    refresh_before_last_user_message_if_needed(session, turn_context).await
}

async fn refresh_before_last_user_message_if_needed(
    session: &Arc<Session>,
    turn_context: &Arc<TurnContext>,
) -> Result<PromptRefreshDecision, TurnError> {
    compaction_refresh::auto_compact_prompt_refresh_if_needed(
        session,
        turn_context,
        InitialContextInjection::BeforeLastUserMessage,
    )
    .await
}
