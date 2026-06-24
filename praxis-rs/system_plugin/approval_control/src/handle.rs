use crate::live::LivePermissions;
use crate::state::ResolvedTurnPermissions;
use tokio::sync::watch;

#[derive(Debug, Clone)]
pub struct PermissionHandle {
    live: LivePermissions,
}

impl PermissionHandle {
    pub(crate) fn new(live: LivePermissions) -> Self {
        Self { live }
    }

    pub fn current(&self) -> ResolvedTurnPermissions {
        self.live.current()
    }

    pub fn subscribe(&self) -> watch::Receiver<ResolvedTurnPermissions> {
        self.live.subscribe()
    }
}
