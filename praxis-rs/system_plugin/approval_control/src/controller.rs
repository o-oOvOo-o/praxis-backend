use crate::handle::PermissionHandle;
use crate::live::LivePermissions;
use crate::resolve::PermissionOverride;
use crate::resolve::apply_permission_override;
use crate::state::PermissionStateSource;
use crate::state::ResolvedTurnPermissions;
use crate::state::ThreadPermissionState;
use crate::store::ApprovalCache;
use crate::store::PendingApprovalStore;
use std::path::Path;
use std::sync::Arc;
use std::sync::Mutex;

#[derive(Debug, Clone)]
pub struct PermissionController {
    inner: Arc<PermissionControllerInner>,
}

#[derive(Debug)]
struct PermissionControllerInner {
    state: Mutex<ThreadPermissionState>,
    live: LivePermissions,
    pending: Mutex<PendingApprovalStore>,
    cache: Mutex<ApprovalCache>,
}

impl PermissionController {
    pub fn new(initial: ThreadPermissionState) -> Self {
        let initial = initial.normalized();
        let (live, _rx) = LivePermissions::new(initial.resolved());
        Self {
            inner: Arc::new(PermissionControllerInner {
                state: Mutex::new(initial),
                live,
                pending: Mutex::new(PendingApprovalStore::default()),
                cache: Mutex::new(ApprovalCache::default()),
            }),
        }
    }

    pub fn handle(&self) -> PermissionHandle {
        PermissionHandle::new(self.inner.live.clone())
    }

    pub fn current(&self) -> ResolvedTurnPermissions {
        self.inner.live.current()
    }

    pub fn current_thread_state(&self) -> ThreadPermissionState {
        self.inner
            .state
            .lock()
            .unwrap_or_else(|err| err.into_inner())
            .clone()
    }

    pub fn replace(&self, next: ThreadPermissionState) -> ResolvedTurnPermissions {
        let mut guard = self
            .inner
            .state
            .lock()
            .unwrap_or_else(|err| err.into_inner());
        let mut next = next.normalized();
        if next.thread_id.is_none() {
            next.thread_id = guard.thread_id.clone();
        }

        let changed = !guard.same_effective_permissions(&next)
            || guard.thread_id != next.thread_id
            || guard.source != next.source;
        next.generation = if changed {
            guard.generation.saturating_add(1)
        } else {
            guard.generation
        };
        if changed {
            self.clear_runtime_approvals();
        }

        *guard = next;
        let resolved = guard.resolved();
        drop(guard);
        self.inner.live.update(resolved.clone());
        resolved
    }

    pub fn apply_override(
        &self,
        override_state: &PermissionOverride,
        cwd: &Path,
    ) -> ResolvedTurnPermissions {
        let current = self.current_thread_state();
        let next = apply_permission_override(&current, override_state, cwd);
        self.replace(next)
    }

    pub fn fork(&self, thread_id: impl Into<String>) -> ResolvedTurnPermissions {
        let next = self
            .current_thread_state()
            .with_thread_id(thread_id)
            .bump(PermissionStateSource::Fork);
        self.replace(next)
    }

    pub fn resume(&self, thread_id: impl Into<String>) -> ResolvedTurnPermissions {
        let next = self
            .current_thread_state()
            .with_thread_id(thread_id)
            .bump(PermissionStateSource::Resume);
        self.replace(next)
    }

    pub fn clear_runtime_approvals(&self) {
        self.inner
            .pending
            .lock()
            .unwrap_or_else(|err| err.into_inner())
            .clear();
        self.inner
            .cache
            .lock()
            .unwrap_or_else(|err| err.into_inner())
            .clear();
    }
}
