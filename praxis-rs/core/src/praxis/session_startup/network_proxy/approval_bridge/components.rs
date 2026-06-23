use std::sync::Arc;

use praxis_network_proxy::BlockedRequestObserver;
use praxis_network_proxy::NetworkPolicyDecider;

use crate::config::Config;
use crate::tools::network_approval::NetworkApprovalService;
use crate::tools::network_approval::build_blocked_request_observer;
use crate::tools::network_approval::build_network_policy_decider;

use super::session_binding::PolicyDeciderSession;

pub(super) fn blocked_request_observer(
    config: &Config,
    network_approval: &Arc<NetworkApprovalService>,
    policy_decider_session: &PolicyDeciderSession,
) -> Option<Arc<dyn BlockedRequestObserver>> {
    if policy_decider_session.is_none() || !config.managed_network_requirements_enabled() {
        return None;
    }

    config
        .permissions
        .network
        .as_ref()
        .map(|_| build_blocked_request_observer(Arc::clone(network_approval)))
}

pub(super) fn network_policy_decider(
    network_approval: &Arc<NetworkApprovalService>,
    policy_decider_session: &PolicyDeciderSession,
) -> Option<Arc<dyn NetworkPolicyDecider>> {
    policy_decider_session
        .as_ref()
        .map(|policy_decider_session| {
            build_network_policy_decider(
                Arc::clone(network_approval),
                Arc::clone(policy_decider_session),
            )
        })
}
