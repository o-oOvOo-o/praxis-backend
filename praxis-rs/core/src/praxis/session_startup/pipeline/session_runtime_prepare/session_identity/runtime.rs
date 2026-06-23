use std::sync::Arc;

use praxis_network_proxy::NetworkProxyAuditMetadata;
use praxis_otel::SessionTelemetry;
use tokio::sync::watch;

use crate::shell::Shell;
use crate::shell_snapshot::ShellSnapshot;

pub(in crate::praxis::session_startup::pipeline::session_runtime_prepare) struct SessionIdentityRuntime
{
    pub(in crate::praxis::session_startup::pipeline::session_runtime_prepare) session_telemetry:
        SessionTelemetry,
    pub(in crate::praxis::session_startup::pipeline::session_runtime_prepare) network_proxy_audit_metadata:
        NetworkProxyAuditMetadata,
    pub(in crate::praxis::session_startup::pipeline::session_runtime_prepare) default_shell: Shell,
    pub(in crate::praxis::session_startup::pipeline::session_runtime_prepare) shell_snapshot_tx:
        watch::Sender<Option<Arc<ShellSnapshot>>>,
}
