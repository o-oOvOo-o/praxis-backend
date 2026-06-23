use std::path::PathBuf;
use std::sync::Arc;

use praxis_protocol::protocol::InitialHistory;
use praxis_protocol::protocol::SessionSource;

use crate::agent::AgentControl;
use crate::config::Config;
use crate::error::Result as PraxisResult;
use crate::exec_policy::ExecPolicyManager;
use crate::rollout::RolloutRecorder;
use crate::shell_snapshot::ShellSnapshot;

use super::super::super::super::ThreadManagerInner;
use super::super::super::super::ThreadSpawnResult;
use super::super::super::spawn_request::ThreadSpawnRequest;

impl ThreadManagerInner {
    pub(crate) async fn resume_thread_from_rollout_with_source(
        &self,
        config: Config,
        rollout_path: PathBuf,
        agent_control: AgentControl,
        session_source: SessionSource,
        inherited_shell_snapshot: Option<Arc<ShellSnapshot>>,
        inherited_exec_policy: Option<Arc<ExecPolicyManager>>,
    ) -> PraxisResult<ThreadSpawnResult> {
        let initial_history = RolloutRecorder::get_rollout_history(&rollout_path).await?;
        let request = ThreadSpawnRequest::new(
            config,
            initial_history,
            Arc::clone(&self.auth_manager),
            agent_control,
            session_source,
        )
        .with_inherited_runtime(inherited_shell_snapshot, inherited_exec_policy);
        Box::pin(self.spawn_from_request(request)).await
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn fork_thread_with_source(
        &self,
        config: Config,
        initial_history: InitialHistory,
        agent_control: AgentControl,
        session_source: SessionSource,
        persist_extended_history: bool,
        inherited_shell_snapshot: Option<Arc<ShellSnapshot>>,
        inherited_exec_policy: Option<Arc<ExecPolicyManager>>,
    ) -> PraxisResult<ThreadSpawnResult> {
        let request = ThreadSpawnRequest::new(
            config,
            initial_history,
            Arc::clone(&self.auth_manager),
            agent_control,
            session_source,
        )
        .with_persist_extended_history(persist_extended_history)
        .with_inherited_runtime(inherited_shell_snapshot, inherited_exec_policy);
        Box::pin(self.spawn_from_request(request)).await
    }
}
