use crate::state::ResolvedTurnPermissions;
use tokio::sync::watch;

#[derive(Debug, Clone)]
pub struct LivePermissions {
    tx: watch::Sender<ResolvedTurnPermissions>,
}

impl LivePermissions {
    pub fn new(
        initial: ResolvedTurnPermissions,
    ) -> (Self, watch::Receiver<ResolvedTurnPermissions>) {
        let (tx, rx) = watch::channel(initial);
        (Self { tx }, rx)
    }

    pub fn subscribe(&self) -> watch::Receiver<ResolvedTurnPermissions> {
        self.tx.subscribe()
    }

    pub fn current(&self) -> ResolvedTurnPermissions {
        self.tx.borrow().clone()
    }

    pub fn update(&self, next: ResolvedTurnPermissions) {
        let _ = self.tx.send(next);
    }
}
