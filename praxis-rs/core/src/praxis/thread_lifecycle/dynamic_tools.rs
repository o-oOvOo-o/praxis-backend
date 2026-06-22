use praxis_protocol::dynamic_tools::DynamicToolSpec;
use praxis_protocol::protocol::InitialHistory;
use praxis_rollout::state_db;

use crate::config::Config;

pub(super) async fn resolve_for_session(
    config: &Config,
    conversation_history: &InitialHistory,
    dynamic_tools: Vec<DynamicToolSpec>,
) -> Vec<DynamicToolSpec> {
    if !dynamic_tools.is_empty() {
        return dynamic_tools;
    }
    persisted_for_history(config, conversation_history)
        .await
        .or_else(|| conversation_history.get_dynamic_tools())
        .unwrap_or_default()
}

async fn persisted_for_history(
    config: &Config,
    conversation_history: &InitialHistory,
) -> Option<Vec<DynamicToolSpec>> {
    let thread_id = match conversation_history {
        InitialHistory::Resumed(resumed) => Some(resumed.conversation_id),
        InitialHistory::Forked(_) => conversation_history.forked_from_id(),
        InitialHistory::New => None,
    }?;
    let state_db_ctx = state_db::get_state_db(config).await;
    state_db::get_dynamic_tools(state_db_ctx.as_deref(), thread_id, "praxis_spawn").await
}
