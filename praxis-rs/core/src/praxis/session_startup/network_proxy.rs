mod approval_bridge;
mod managed_proxy;

use praxis_network_proxy::NetworkProxyAuditMetadata;
use praxis_protocol::protocol::SessionNetworkProxyRuntime;

use crate::config::Config;
use crate::config::StartedNetworkProxy;
use crate::exec_policy::ExecPolicyManager;
use crate::tools::network_approval::NetworkApprovalService;

pub(super) use approval_bridge::PolicyDeciderSession;
pub(super) use approval_bridge::bind_session;

use std::sync::Arc;

pub(super) struct NetworkBootstrap {
    pub(super) network_proxy: Option<StartedNetworkProxy>,
    pub(super) session_network_proxy: Option<SessionNetworkProxyRuntime>,
    pub(super) network_approval: Arc<NetworkApprovalService>,
    pub(super) policy_decider_session: approval_bridge::PolicyDeciderSession,
}

pub(super) async fn start(
    config: &Config,
    exec_policy: &ExecPolicyManager,
    audit_metadata: NetworkProxyAuditMetadata,
) -> anyhow::Result<NetworkBootstrap> {
    let approval = approval_bridge::build(config);
    let (network_proxy, session_network_proxy) =
        managed_proxy::start_if_configured(config, exec_policy, audit_metadata, &approval).await?;

    Ok(NetworkBootstrap {
        network_proxy,
        session_network_proxy,
        network_approval: approval.network_approval,
        policy_decider_session: approval.policy_decider_session,
    })
}
