use crate::state::PermissionStateSource;
use crate::state::ThreadPermissionState;

pub fn fork_permissions(
    source: &ThreadPermissionState,
    new_thread_id: impl Into<String>,
) -> ThreadPermissionState {
    let mut next = source.clone().bump(PermissionStateSource::Fork);
    next.thread_id = Some(new_thread_id.into());
    next
}
