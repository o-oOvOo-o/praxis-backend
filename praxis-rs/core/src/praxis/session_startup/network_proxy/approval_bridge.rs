use std::sync::Arc;

use crate::config::Config;
use crate::tools::network_approval::NetworkApprovalService;
use praxis_network_proxy::BlockedRequestObserver;
use praxis_network_proxy::NetworkPolicyDecider;

mod components;
mod session_binding;

pub(in crate::praxis::session_startup) use session_binding::PolicyDeciderSession;
pub(in crate::praxis::session_startup) use session_binding::bind_session;

pub(super) struct NetworkApprovalBridge {
    pub(super) network_approval: Arc<NetworkApprovalService>,
    pub(super) policy_decider_session: PolicyDeciderSession,
    pub(super) blocked_request_observer: Option<Arc<dyn BlockedRequestObserver>>,
    pub(super) network_policy_decider: Option<Arc<dyn NetworkPolicyDecider>>,
}

pub(super) fn build(config: &Config) -> NetworkApprovalBridge {
    let network_approval = Arc::new(NetworkApprovalService::default());
    let policy_decider_session = session_binding::new_policy_decider_session(config);
    let blocked_request_observer =
        components::blocked_request_observer(config, &network_approval, &policy_decider_session);
    let network_policy_decider =
        components::network_policy_decider(&network_approval, &policy_decider_session);

    NetworkApprovalBridge {
        network_approval,
        policy_decider_session,
        blocked_request_observer,
        network_policy_decider,
    }
}
