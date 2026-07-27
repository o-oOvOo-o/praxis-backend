use std::sync::Arc;
use std::time::Duration;

use praxis_protocol::ThreadId;
use praxis_rollout::state_db::StateDbHandle;
use tracing::warn;

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
    const AGENT_OS_STARTUP_TIMEOUT: Duration = Duration::from_secs(2);

    let registration_agent_os = Arc::clone(agent_os);
    let registration_state_db = state_db_ctx.clone();
    let registration_session_configuration = session_configuration.clone();
    let registration_runtime = tokio::runtime::Handle::current();
    let mut registration = tokio::task::spawn_blocking(move || {
        registration_runtime.block_on(agent_os_bootstrap::register_session_thread(
            &registration_agent_os,
            registration_state_db,
            conversation_id,
            &registration_session_configuration,
        ))
    });
    match tokio::time::timeout(AGENT_OS_STARTUP_TIMEOUT, &mut registration).await {
        Ok(result) => result??,
        Err(_) => {
            registration.abort();
            warn!(
                %conversation_id,
                timeout_ms = AGENT_OS_STARTUP_TIMEOUT.as_millis(),
                "AgentOS bootstrap timed out; continuing session startup with canonical in-memory state"
            );
        }
    }

    let unified_exec_manager = Arc::new(UnifiedExecProcessManager::new(
        background_terminal_max_timeout,
    ));
    agent_os_bootstrap::attach_process_cleaners(agent_os, Arc::clone(&unified_exec_manager)).await;
    Ok(unified_exec_manager)
}
