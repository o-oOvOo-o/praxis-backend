use std::sync::Arc;

use praxis_login::AuthManager;
use praxis_protocol::dynamic_tools::DynamicToolSpec;
use praxis_protocol::protocol::InitialHistory;
use praxis_protocol::protocol::SessionSource;
use praxis_protocol::protocol::W3cTraceContext;

use crate::agent::AgentControl;
use crate::config::Config;
use crate::exec_policy::ExecPolicyManager;
use crate::shell::Shell;
use crate::shell_snapshot::ShellSnapshot;

pub(super) struct ThreadSpawnRequest {
    pub(super) config: Config,
    pub(super) initial_history: InitialHistory,
    pub(super) auth_manager: Arc<AuthManager>,
    pub(super) agent_control: AgentControl,
    pub(super) session_source: SessionSource,
    pub(super) dynamic_tools: Vec<DynamicToolSpec>,
    pub(super) persist_extended_history: bool,
    pub(super) metrics_service_name: Option<String>,
    pub(super) inherited_shell_snapshot: Option<Arc<ShellSnapshot>>,
    pub(super) inherited_exec_policy: Option<Arc<ExecPolicyManager>>,
    pub(super) parent_trace: Option<W3cTraceContext>,
    pub(super) user_shell_override: Option<Shell>,
}

impl ThreadSpawnRequest {
    pub(super) fn new(
        config: Config,
        initial_history: InitialHistory,
        auth_manager: Arc<AuthManager>,
        agent_control: AgentControl,
        session_source: SessionSource,
    ) -> Self {
        Self {
            config,
            initial_history,
            auth_manager,
            agent_control,
            session_source,
            dynamic_tools: Vec::new(),
            persist_extended_history: false,
            metrics_service_name: None,
            inherited_shell_snapshot: None,
            inherited_exec_policy: None,
            parent_trace: None,
            user_shell_override: None,
        }
    }

    pub(super) fn with_dynamic_tools(mut self, dynamic_tools: Vec<DynamicToolSpec>) -> Self {
        self.dynamic_tools = dynamic_tools;
        self
    }

    pub(super) fn with_persist_extended_history(mut self, persist_extended_history: bool) -> Self {
        self.persist_extended_history = persist_extended_history;
        self
    }

    pub(super) fn with_metrics_service_name(
        mut self,
        metrics_service_name: Option<String>,
    ) -> Self {
        self.metrics_service_name = metrics_service_name;
        self
    }

    pub(super) fn with_inherited_runtime(
        mut self,
        inherited_shell_snapshot: Option<Arc<ShellSnapshot>>,
        inherited_exec_policy: Option<Arc<ExecPolicyManager>>,
    ) -> Self {
        self.inherited_shell_snapshot = inherited_shell_snapshot;
        self.inherited_exec_policy = inherited_exec_policy;
        self
    }

    pub(super) fn with_parent_trace(mut self, parent_trace: Option<W3cTraceContext>) -> Self {
        self.parent_trace = parent_trace;
        self
    }

    pub(super) fn with_user_shell_override(mut self, user_shell_override: Option<Shell>) -> Self {
        self.user_shell_override = user_shell_override;
        self
    }
}
