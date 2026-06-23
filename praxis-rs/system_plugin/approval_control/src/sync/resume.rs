use crate::state::PermissionStateSource;
use crate::state::ThreadPermissionState;

pub fn resume_permissions(
    source: &ThreadPermissionState,
    thread_id: impl Into<String>,
) -> ThreadPermissionState {
    let mut next = source.clone().bump(PermissionStateSource::Resume);
    next.thread_id = Some(thread_id.into());
    next
}
