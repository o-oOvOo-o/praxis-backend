use praxis_rollout::state_db;

use crate::praxis::Session;
use crate::praxis::TurnContext;

pub(super) async fn maybe_mark_thread_memory_mode_polluted(
    sess: &Session,
    turn_context: &TurnContext,
) {
    if !turn_context
        .config
        .memories
        .no_memories_if_mcp_or_web_search
    {
        return;
    }
    state_db::mark_thread_memory_mode_polluted(
        sess.services.state_db.as_deref(),
        sess.conversation_id,
        "mcp_tool_call",
    )
    .await;
}
