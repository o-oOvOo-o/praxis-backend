use std::sync::Arc;

use praxis_protocol::ThreadId;
use praxis_state::StateRuntime;

use crate::agent_os::AgentOs;
use crate::agent_os::ThreadRegistration;
use crate::agent_os::coordination_scope_for_session_source;
use crate::agent_os::profile_for_rank;
use crate::agent_os::rank_for_session_source;
use crate::praxis::SessionConfiguration;

pub(in crate::praxis::session_startup) async fn register_session_thread(
    agent_os: &Arc<AgentOs>,
    state_db_ctx: Option<Arc<StateRuntime>>,
    conversation_id: ThreadId,
    session_configuration: &SessionConfiguration,
) -> anyhow::Result<()> {
    agent_os.attach_state_db(state_db_ctx).await;
    let agent_rank = rank_for_session_source(&session_configuration.session_source);
    agent_os
        .register_thread(ThreadRegistration {
            thread_id: conversation_id,
            coordination_scope: coordination_scope_for_session_source(
                &session_configuration.session_source,
                conversation_id,
            ),
            rank: agent_rank,
            profile_id: profile_for_rank(agent_rank).to_string(),
            cwd: session_configuration.cwd.to_path_buf(),
            repo_id: None,
            branch: None,
            worktree: None,
            priority: if agent_rank == 0 { 100 } else { 0 },
        })
        .await?;
    agent_os
        .ensure_bootstrap_task(
            conversation_id,
            "Session bootstrap task",
            vec![session_configuration.cwd.display().to_string()],
        )
        .await?;
    Ok(())
}
