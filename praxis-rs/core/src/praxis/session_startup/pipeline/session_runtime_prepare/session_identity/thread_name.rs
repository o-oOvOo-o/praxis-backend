use std::sync::Arc;

use praxis_protocol::ThreadId;
use praxis_protocol::protocol::InitialHistory;
use praxis_rollout::state_db::StateDbHandle;

use crate::config::Config;
use crate::praxis::SessionConfiguration;

use super::super::super::super::thread_name_bootstrap;

pub(super) struct ThreadNameInput<'a> {
    pub(super) conversation_id: ThreadId,
    pub(super) forked_from_id: Option<ThreadId>,
    pub(super) initial_history: &'a InitialHistory,
    pub(super) state_db_ctx: &'a Option<StateDbHandle>,
    pub(super) config: &'a Arc<Config>,
    pub(super) session_configuration: &'a mut SessionConfiguration,
}

pub(super) async fn resolve_and_assign(input: ThreadNameInput<'_>) {
    let thread_name = thread_name_bootstrap::resolve_session_thread_name(
        input.conversation_id,
        input.forked_from_id,
        input.initial_history,
        input.state_db_ctx.as_deref(),
        input.config.ephemeral,
    )
    .await;
    input.session_configuration.thread_name = thread_name;
}
