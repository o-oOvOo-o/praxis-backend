use std::sync::Arc;

use praxis_hooks::Hooks;
use praxis_otel::SessionTelemetry;
use praxis_protocol::protocol::SessionNetworkProxyRuntime;
use tokio::sync::watch;

use crate::config::StartedNetworkProxy;
use crate::shell::Shell;
use crate::shell_snapshot::ShellSnapshot;
use crate::tools::network_approval::NetworkApprovalService;
use crate::unified_exec::UnifiedExecProcessManager;

use super::super::super::super::network_proxy;

pub(in crate::praxis::session_startup::pipeline) struct SessionRuntimePreparation {
    pub(in crate::praxis::session_startup::pipeline) session_telemetry: SessionTelemetry,
    pub(in crate::praxis::session_startup::pipeline) default_shell: Shell,
    pub(in crate::praxis::session_startup::pipeline) shell_snapshot_tx:
        watch::Sender<Option<Arc<ShellSnapshot>>>,
    pub(in crate::praxis::session_startup::pipeline) started_network_proxy:
        Option<StartedNetworkProxy>,
    pub(in crate::praxis::session_startup::pipeline) session_network_proxy:
        Option<SessionNetworkProxyRuntime>,
    pub(in crate::praxis::session_startup::pipeline) network_approval: Arc<NetworkApprovalService>,
    pub(in crate::praxis::session_startup::pipeline) network_policy_decider_session:
        network_proxy::PolicyDeciderSession,
    pub(in crate::praxis::session_startup::pipeline) hooks: Hooks,
    pub(in crate::praxis::session_startup::pipeline) unified_exec_manager:
        Arc<UnifiedExecProcessManager>,
}
