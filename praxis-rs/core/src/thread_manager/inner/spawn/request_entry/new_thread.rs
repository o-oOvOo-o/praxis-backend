use std::sync::Arc;

use praxis_protocol::dynamic_tools::DynamicToolSpec;
use praxis_protocol::ThreadId;
use praxis_protocol::protocol::InitialHistory;
use praxis_protocol::protocol::SessionSource;
use praxis_protocol::protocol::W3cTraceContext;

use crate::agent::AgentControl;
use crate::config::Config;
use crate::error::Result as PraxisResult;
use crate::exec_policy::ExecPolicyManager;
use crate::shell_snapshot::ShellSnapshot;

use super::super::super::super::ThreadManagerInner;
use super::super::super::super::ThreadSpawnResult;
use super::super::super::spawn_request::ThreadSpawnRequest;

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
            Vec::new(),
            /*persist_extended_history*/ false,
            /*metrics_service_name*/ None,
            /*parent_trace*/ None,
            /*inherited_shell_snapshot*/ None,
            /*inherited_exec_policy*/ None,
            /*requested_thread_id*/ None,
        ))
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn spawn_new_thread_with_source(
        &self,
        config: Config,
        agent_control: AgentControl,
        session_source: SessionSource,
        dynamic_tools: Vec<DynamicToolSpec>,
        persist_extended_history: bool,
        metrics_service_name: Option<String>,
        parent_trace: Option<W3cTraceContext>,
        inherited_shell_snapshot: Option<Arc<ShellSnapshot>>,
        inherited_exec_policy: Option<Arc<ExecPolicyManager>>,
        requested_thread_id: Option<ThreadId>,
    ) -> PraxisResult<ThreadSpawnResult> {
        let request = ThreadSpawnRequest::new(
            config,
            InitialHistory::New,
            Arc::clone(&self.auth_manager),
            agent_control,
            session_source,
        )
        .with_requested_thread_id(requested_thread_id)
        .with_dynamic_tools(dynamic_tools)
        .with_persist_extended_history(persist_extended_history)
        .with_metrics_service_name(metrics_service_name)
        .with_parent_trace(parent_trace)
        .with_inherited_runtime(inherited_shell_snapshot, inherited_exec_policy);
        Box::pin(self.spawn_from_request(request)).await
    }
}
