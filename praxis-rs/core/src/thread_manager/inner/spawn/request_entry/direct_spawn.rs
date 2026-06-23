use std::sync::Arc;

use praxis_login::AuthManager;
use praxis_protocol::dynamic_tools::DynamicToolSpec;
use praxis_protocol::protocol::InitialHistory;
use praxis_protocol::protocol::SessionSource;
use praxis_protocol::protocol::W3cTraceContext;

use crate::agent::AgentControl;
use crate::config::Config;
use crate::error::Result as PraxisResult;
use crate::exec_policy::ExecPolicyManager;
use crate::shell::Shell;
use crate::shell_snapshot::ShellSnapshot;

use super::super::super::super::ThreadManagerInner;
use super::super::super::super::ThreadSpawnResult;
use super::super::super::spawn_request::ThreadSpawnRequest;

impl ThreadManagerInner {
    /// Spawn a new thread with optional history and register it with the manager.
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn spawn_thread(
        &self,
        config: Config,
        initial_history: InitialHistory,
        auth_manager: Arc<AuthManager>,
        agent_control: AgentControl,
        dynamic_tools: Vec<DynamicToolSpec>,
        persist_extended_history: bool,
        metrics_service_name: Option<String>,
        parent_trace: Option<W3cTraceContext>,
        user_shell_override: Option<Shell>,
    ) -> PraxisResult<ThreadSpawnResult> {
        let request = ThreadSpawnRequest::new(
            config,
            initial_history,
            auth_manager,
            agent_control,
            self.session_source.clone(),
        )
        .with_dynamic_tools(dynamic_tools)
        .with_persist_extended_history(persist_extended_history)
        .with_metrics_service_name(metrics_service_name)
        .with_parent_trace(parent_trace)
        .with_user_shell_override(user_shell_override);
        Box::pin(self.spawn_from_request(request)).await
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn spawn_thread_with_source(
        &self,
        config: Config,
        initial_history: InitialHistory,
        auth_manager: Arc<AuthManager>,
        agent_control: AgentControl,
        session_source: SessionSource,
        dynamic_tools: Vec<DynamicToolSpec>,
        persist_extended_history: bool,
        metrics_service_name: Option<String>,
        inherited_shell_snapshot: Option<Arc<ShellSnapshot>>,
        inherited_exec_policy: Option<Arc<ExecPolicyManager>>,
        parent_trace: Option<W3cTraceContext>,
        user_shell_override: Option<Shell>,
    ) -> PraxisResult<ThreadSpawnResult> {
        let request = ThreadSpawnRequest::new(
            config,
            initial_history,
            auth_manager,
            agent_control,
            session_source,
        )
        .with_dynamic_tools(dynamic_tools)
        .with_persist_extended_history(persist_extended_history)
        .with_metrics_service_name(metrics_service_name)
        .with_inherited_runtime(inherited_shell_snapshot, inherited_exec_policy)
        .with_parent_trace(parent_trace)
        .with_user_shell_override(user_shell_override);
        Box::pin(self.spawn_from_request(request)).await
    }
}
