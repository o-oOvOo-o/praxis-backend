use std::sync::Arc;

use praxis_loop::decisions::ContextPressureDecision;

use crate::compact::InitialContextInjection;

use super::super::super::Session;
use super::super::super::TurnContext;
use super::super::compaction_refresh;

pub(in crate::praxis::turn_loop_adapter) async fn context_pressure_decision(
    session: &Arc<Session>,
    turn_context: &Arc<TurnContext>,
) -> ContextPressureDecision {
    if !compaction_refresh::auto_compact_needed(session, turn_context).await {
        return ContextPressureDecision::Proceed;
    }

    match compaction_refresh::auto_compact_prompt_refresh(
        session,
        turn_context,
        InitialContextInjection::DoNotInject,
    )
    .await
    {
        Ok(prompt_items) => ContextPressureDecision::Compacted {
            prompt_items,
            transcript_items: Vec::new(),
        },
        Err(err) => ContextPressureDecision::Abort(err),
    }
}
