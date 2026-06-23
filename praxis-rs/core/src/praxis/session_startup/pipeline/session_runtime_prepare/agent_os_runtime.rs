use std::sync::Arc;

use praxis_protocol::ThreadId;
use praxis_rollout::state_db::StateDbHandle;

use crate::agent_os::AgentOs;
use crate::praxis::SessionConfiguration;
use crate::unified_exec::UnifiedExecProcessManager;

use super::super::super::agent_os_bootstrap;

pub(super) async fn register_and_attach(
    agent_os: &Arc<AgentOs>,
    state_db_ctx: &Option<StateDbHandle>,
    conversation_id: ThreadId,
    session_configuration: &SessionConfiguration,
    background_terminal_max_timeout: u64,
) -> anyhow::Result<Arc<UnifiedExecProcessManager>> {
    agent_os_bootstrap::register_session_thread(
        agent_os,
        state_db_ctx.clone(),
        conversation_id,
        session_configuration,
    )
    .await?;

    let unified_exec_manager = Arc::new(UnifiedExecProcessManager::new(
        background_terminal_max_timeout,
    ));
    agent_os_bootstrap::attach_process_cleaners(agent_os, Arc::clone(&unified_exec_manager)).await;
    Ok(unified_exec_manager)
}
