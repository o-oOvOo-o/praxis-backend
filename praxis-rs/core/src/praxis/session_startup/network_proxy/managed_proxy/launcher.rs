use std::sync::Arc;

use praxis_network_proxy::BlockedRequestObserver;
use praxis_network_proxy::NetworkPolicyDecider;
use praxis_network_proxy::NetworkProxyAuditMetadata;
use praxis_protocol::protocol::SandboxPolicy;

use crate::config::NetworkProxySpec;
use crate::config::StartedNetworkProxy;

use super::policy_spec;

pub(super) struct ManagedProxyStartInput<'a> {
    pub(super) spec: &'a NetworkProxySpec,
    pub(super) exec_policy: &'a praxis_execpolicy::Policy,
    pub(super) sandbox_policy: &'a SandboxPolicy,
    pub(super) network_policy_decider: Option<Arc<dyn NetworkPolicyDecider>>,
    pub(super) blocked_request_observer: Option<Arc<dyn BlockedRequestObserver>>,
    pub(super) managed_network_requirements_enabled: bool,
    pub(super) audit_metadata: NetworkProxyAuditMetadata,
}

pub(super) async fn start(
    input: ManagedProxyStartInput<'_>,
) -> anyhow::Result<StartedNetworkProxy> {
    let spec = policy_spec::with_exec_policy_network_rules(input.spec, input.exec_policy);
    spec.start_proxy(
        input.sandbox_policy,
        input.network_policy_decider,
        input.blocked_request_observer,
        input.managed_network_requirements_enabled,
        input.audit_metadata,
    )
    .await
    .map_err(|err| anyhow::anyhow!("failed to start managed network proxy: {err}"))
}
