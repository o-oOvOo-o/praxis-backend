use std::sync::Arc;

use praxis_network_proxy::NetworkProxyAuditMetadata;
use praxis_protocol::protocol::SessionNetworkProxyRuntime;
use tracing::Instrument;
use tracing::info_span;

use crate::config::Config;
use crate::config::StartedNetworkProxy;
use crate::exec_policy::ExecPolicyManager;

use super::approval_bridge::NetworkApprovalBridge;

mod launcher;
mod policy_spec;
mod runtime_projection;

pub(super) async fn start_if_configured(
    config: &Config,
    exec_policy: &ExecPolicyManager,
    audit_metadata: NetworkProxyAuditMetadata,
    approval: &NetworkApprovalBridge,
) -> anyhow::Result<(
    Option<StartedNetworkProxy>,
    Option<SessionNetworkProxyRuntime>,
)> {
    let Some(spec) = config.permissions.network.as_ref() else {
        return Ok((None, None));
    };

    let managed_network_requirements_enabled = config.managed_network_requirements_enabled();
    let current_exec_policy = exec_policy.current();
    let network_proxy = launcher::start(launcher::ManagedProxyStartInput {
        spec,
        exec_policy: current_exec_policy.as_ref(),
        sandbox_policy: config.permissions.sandbox_policy.get(),
        network_policy_decider: approval.network_policy_decider.as_ref().map(Arc::clone),
        blocked_request_observer: approval.blocked_request_observer.as_ref().map(Arc::clone),
        managed_network_requirements_enabled,
        audit_metadata,
    })
    .instrument(info_span!(
        "session_init.network_proxy",
        otel.name = "session_init.network_proxy",
        session_init.managed_network_requirements_enabled = managed_network_requirements_enabled,
    ))
    .await?;
    let session_network_proxy = runtime_projection::from_started_proxy(&network_proxy);

    Ok((Some(network_proxy), Some(session_network_proxy)))
}
