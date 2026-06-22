use std::sync::Arc;

use super::super::super::Session;
use super::super::super::TurnContext;
use super::super::super::turn_compaction::effective_auto_compact_token_limit;

pub(in crate::praxis::turn_loop_adapter) async fn auto_compact_needed(
    session: &Arc<Session>,
    turn_context: &Arc<TurnContext>,
) -> bool {
    let total_usage_tokens = session.get_total_token_usage().await;
    total_usage_tokens >= auto_compact_limit_or_max(session, turn_context)
}

fn auto_compact_limit_or_max(session: &Session, turn_context: &TurnContext) -> i64 {
    effective_auto_compact_token_limit(session, turn_context).unwrap_or(i64::MAX)
}
