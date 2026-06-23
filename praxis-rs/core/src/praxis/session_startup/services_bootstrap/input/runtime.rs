use std::sync::Arc;

use praxis_hooks::Hooks;
use praxis_otel::SessionTelemetry;
use praxis_rollout::state_db::StateDbHandle;
use tokio::sync::watch;

use crate::config::StartedNetworkProxy;
use crate::exec_policy::ExecPolicyManager;
use crate::rollout::RolloutRecorder;
use crate::shell::Shell;
use crate::shell_snapshot::ShellSnapshot;
use crate::tools::network_approval::NetworkApprovalService;
use crate::unified_exec::UnifiedExecProcessManager;

pub(in crate::praxis::session_startup) struct ServiceRuntimeArtifacts {
    pub(in crate::praxis::session_startup) exec_policy: Arc<ExecPolicyManager>,
    pub(in crate::praxis::session_startup) hooks: Hooks,
    pub(in crate::praxis::session_startup) rollout_recorder: Option<RolloutRecorder>,
    pub(in crate::praxis::session_startup) default_shell: Shell,
    pub(in crate::praxis::session_startup) shell_snapshot_tx:
        watch::Sender<Option<Arc<ShellSnapshot>>>,
    pub(in crate::praxis::session_startup) session_telemetry: SessionTelemetry,
    pub(in crate::praxis::session_startup) started_network_proxy: Option<StartedNetworkProxy>,
    pub(in crate::praxis::session_startup) network_approval: Arc<NetworkApprovalService>,
    pub(in crate::praxis::session_startup) state_db_ctx: Option<StateDbHandle>,
    pub(in crate::praxis::session_startup) unified_exec_manager: Arc<UnifiedExecProcessManager>,
}
