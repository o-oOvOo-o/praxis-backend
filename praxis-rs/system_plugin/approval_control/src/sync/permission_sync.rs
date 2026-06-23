use super::fork_permissions;
use super::resume_permissions;
use crate::state::ThreadPermissionState;

#[derive(Debug, Default, Clone)]
pub struct PermissionSync;

impl PermissionSync {
    pub fn fork(
        &self,
        source: &ThreadPermissionState,
        new_thread_id: impl Into<String>,
    ) -> ThreadPermissionState {
        fork_permissions(source, new_thread_id)
    }

    pub fn resume(
        &self,
        source: &ThreadPermissionState,
        thread_id: impl Into<String>,
    ) -> ThreadPermissionState {
        resume_permissions(source, thread_id)
    }
}
