use std::sync::Arc;
use std::sync::Weak;

use crate::config::Config;
use crate::praxis::Session;
use crate::tools::network_approval::NetworkApprovalService;
use crate::tools::network_approval::build_blocked_request_observer;
use crate::tools::network_approval::build_network_policy_decider;
use praxis_network_proxy::BlockedRequestObserver;
use praxis_network_proxy::NetworkPolicyDecider;
use tokio::sync::RwLock;

pub(in crate::praxis::session_startup) type PolicyDeciderSession =
    Option<Arc<RwLock<Weak<Session>>>>;

pub(super) struct NetworkApprovalBridge {
    pub(super) network_approval: Arc<NetworkApprovalService>,
    pub(super) policy_decider_session: PolicyDeciderSession,
    pub(super) blocked_request_observer: Option<Arc<dyn BlockedRequestObserver>>,
    pub(super) network_policy_decider: Option<Arc<dyn NetworkPolicyDecider>>,
}

pub(super) fn build(config: &Config) -> NetworkApprovalBridge {
    let network_approval = Arc::new(NetworkApprovalService::default());
    let policy_decider_session = policy_decider_session(config);
    let blocked_request_observer =
        blocked_request_observer(config, &network_approval, &policy_decider_session);
    let network_policy_decider = network_policy_decider(&network_approval, &policy_decider_session);

    NetworkApprovalBridge {
        network_approval,
        policy_decider_session,
        blocked_request_observer,
        network_policy_decider,
    }
}

pub(in crate::praxis::session_startup) async fn bind_session(
    policy_decider_session: PolicyDeciderSession,
    session: &Arc<Session>,
) {
    if let Some(policy_decider_session) = policy_decider_session {
        let mut guard = policy_decider_session.write().await;
        *guard = Arc::downgrade(session);
    }
}

fn policy_decider_session(config: &Config) -> PolicyDeciderSession {
    if !config.managed_network_requirements_enabled() {
        return None;
    }

    config
        .permissions
        .network
        .as_ref()
        .map(|_| Arc::new(RwLock::new(Weak::<Session>::new())))
}

fn blocked_request_observer(
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

fn network_policy_decider(
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
