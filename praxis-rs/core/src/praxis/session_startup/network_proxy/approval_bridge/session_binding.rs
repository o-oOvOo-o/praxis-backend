use std::sync::Arc;
use std::sync::Weak;

use tokio::sync::RwLock;

use crate::config::Config;
use crate::praxis::Session;

pub(in crate::praxis::session_startup) type PolicyDeciderSession =
    Option<Arc<RwLock<Weak<Session>>>>;

pub(super) fn new_policy_decider_session(config: &Config) -> PolicyDeciderSession {
    if !config.managed_network_requirements_enabled() {
        return None;
    }

    config
        .permissions
        .network
        .as_ref()
        .map(|_| Arc::new(RwLock::new(Weak::<Session>::new())))
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
