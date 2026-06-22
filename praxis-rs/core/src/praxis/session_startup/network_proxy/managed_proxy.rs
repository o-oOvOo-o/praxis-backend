use std::sync::Arc;

use praxis_network_proxy::BlockedRequestObserver;
use praxis_network_proxy::NetworkPolicyDecider;
use praxis_network_proxy::NetworkProxyAuditMetadata;
use praxis_protocol::protocol::SandboxPolicy;
use praxis_protocol::protocol::SessionNetworkProxyRuntime;
use tracing::Instrument;
use tracing::info_span;

use crate::config::Config;
use crate::config::NetworkProxySpec;
use crate::config::StartedNetworkProxy;
use crate::exec_policy::ExecPolicyManager;

use super::approval_bridge::NetworkApprovalBridge;

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
    let (network_proxy, session_network_proxy) = start_managed_network_proxy(
        spec,
        current_exec_policy.as_ref(),
        config.permissions.sandbox_policy.get(),
        approval.network_policy_decider.as_ref().map(Arc::clone),
        approval.blocked_request_observer.as_ref().map(Arc::clone),
        managed_network_requirements_enabled,
        audit_metadata,
    )
    .instrument(info_span!(
        "session_init.network_proxy",
        otel.name = "session_init.network_proxy",
        session_init.managed_network_requirements_enabled = managed_network_requirements_enabled,
    ))
    .await?;

    Ok((Some(network_proxy), Some(session_network_proxy)))
}

async fn start_managed_network_proxy(
    spec: &NetworkProxySpec,
    exec_policy: &praxis_execpolicy::Policy,
    sandbox_policy: &SandboxPolicy,
    network_policy_decider: Option<Arc<dyn NetworkPolicyDecider>>,
    blocked_request_observer: Option<Arc<dyn BlockedRequestObserver>>,
    managed_network_requirements_enabled: bool,
    audit_metadata: NetworkProxyAuditMetadata,
) -> anyhow::Result<(StartedNetworkProxy, SessionNetworkProxyRuntime)> {
    let spec = spec
        .with_exec_policy_network_rules(exec_policy)
        .map_err(|err| {
            tracing::warn!(
                "failed to apply execpolicy network rules to managed proxy; continuing with configured network policy: {err}"
            );
            err
        })
        .unwrap_or_else(|_| spec.clone());
    let network_proxy = spec
        .start_proxy(
            sandbox_policy,
            network_policy_decider,
            blocked_request_observer,
            managed_network_requirements_enabled,
            audit_metadata,
        )
        .await
        .map_err(|err| anyhow::anyhow!("failed to start managed network proxy: {err}"))?;
    let session_network_proxy = {
        let proxy = network_proxy.proxy();
        SessionNetworkProxyRuntime {
            http_addr: proxy.http_addr().to_string(),
            socks_addr: proxy.socks_addr().to_string(),
        }
    };
    Ok((network_proxy, session_network_proxy))
}
