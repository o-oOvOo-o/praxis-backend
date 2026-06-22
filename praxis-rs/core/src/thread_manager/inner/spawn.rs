use std::path::PathBuf;
use std::sync::Arc;

use praxis_login::AuthManager;
use praxis_protocol::dynamic_tools::DynamicToolSpec;
use praxis_protocol::protocol::InitialHistory;
use praxis_protocol::protocol::SessionSource;
use praxis_protocol::protocol::W3cTraceContext;

use crate::agent::AgentControl;
use crate::config::Config;
use crate::error::Result as PraxisResult;
use crate::praxis::Praxis;
use crate::praxis::PraxisSpawnArgs;
use crate::praxis::PraxisSpawnOk;
use crate::rollout::RolloutRecorder;
use crate::shell_snapshot::ShellSnapshot;

use super::super::ThreadManagerInner;
use super::super::ThreadSpawnResult;
use super::spawn_request::ThreadSpawnRequest;

impl ThreadManagerInner {
    /// Spawn a new thread with no history using a provided config.
    pub(crate) async fn spawn_new_thread(
        &self,
        config: Config,
        agent_control: AgentControl,
    ) -> PraxisResult<ThreadSpawnResult> {
        Box::pin(self.spawn_new_thread_with_source(
            config,
            agent_control,
            self.session_source.clone(),
            /*persist_extended_history*/ false,
            /*metrics_service_name*/ None,
            /*inherited_shell_snapshot*/ None,
            /*inherited_exec_policy*/ None,
        ))
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn spawn_new_thread_with_source(
        &self,
        config: Config,
        agent_control: AgentControl,
        session_source: SessionSource,
        persist_extended_history: bool,
        metrics_service_name: Option<String>,
        inherited_shell_snapshot: Option<Arc<ShellSnapshot>>,
        inherited_exec_policy: Option<Arc<crate::exec_policy::ExecPolicyManager>>,
    ) -> PraxisResult<ThreadSpawnResult> {
        let request = ThreadSpawnRequest::new(
            config,
            InitialHistory::New,
            Arc::clone(&self.auth_manager),
            agent_control,
            session_source,
        )
        .with_persist_extended_history(persist_extended_history)
        .with_metrics_service_name(metrics_service_name)
        .with_inherited_runtime(inherited_shell_snapshot, inherited_exec_policy);
        Box::pin(self.spawn_from_request(request)).await
    }

    pub(crate) async fn resume_thread_from_rollout_with_source(
        &self,
        config: Config,
        rollout_path: PathBuf,
        agent_control: AgentControl,
        session_source: SessionSource,
        inherited_shell_snapshot: Option<Arc<ShellSnapshot>>,
        inherited_exec_policy: Option<Arc<crate::exec_policy::ExecPolicyManager>>,
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
        inherited_exec_policy: Option<Arc<crate::exec_policy::ExecPolicyManager>>,
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
        user_shell_override: Option<crate::shell::Shell>,
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
        inherited_exec_policy: Option<Arc<crate::exec_policy::ExecPolicyManager>>,
        parent_trace: Option<W3cTraceContext>,
        user_shell_override: Option<crate::shell::Shell>,
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

    async fn spawn_from_request(
        &self,
        request: ThreadSpawnRequest,
    ) -> PraxisResult<ThreadSpawnResult> {
        let watch_registration = self.skills_watcher.register_config(
            &request.config,
            self.skills_manager.as_ref(),
            self.plugins_manager.as_ref(),
        );
        let PraxisSpawnOk {
            praxis, thread_id, ..
        } = Praxis::spawn(PraxisSpawnArgs {
            config: request.config,
            auth_manager: request.auth_manager,
            models_manager: Arc::clone(&self.models_manager),
            environment_manager: Arc::clone(&self.environment_manager),
            skills_manager: Arc::clone(&self.skills_manager),
            plugins_manager: Arc::clone(&self.plugins_manager),
            mcp_manager: Arc::clone(&self.mcp_manager),
            skills_watcher: Arc::clone(&self.skills_watcher),
            conversation_history: request.initial_history,
            session_source: request.session_source,
            agent_control: request.agent_control,
            agent_os: Arc::clone(&self.agent_os),
            dynamic_tools: request.dynamic_tools,
            persist_extended_history: request.persist_extended_history,
            metrics_service_name: request.metrics_service_name,
            inherited_shell_snapshot: request.inherited_shell_snapshot,
            inherited_exec_policy: request.inherited_exec_policy,
            user_shell_override: request.user_shell_override,
            parent_trace: request.parent_trace,
        })
        .await?;
        self.finalize_thread_spawn(praxis, thread_id, watch_registration)
            .await
    }
}
