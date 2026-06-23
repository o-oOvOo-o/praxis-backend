use super::super::*;

pub(super) fn mark_command_process_finished(
    state: &mut crate::agent_os::state::AgentOsState,
    process_ref: Option<(i32, Option<String>)>,
    now: chrono::DateTime<Utc>,
) {
    let Some((process_id, runtime_owner_id)) = process_ref else {
        return;
    };
    let process_key = process_registry_key(process_id, runtime_owner_id.as_deref());
    if let Some(process) = state.processes.get_mut(process_key.as_str()) {
        process.last_heartbeat = now;
        process.ended_at = Some(now);
        process.status = ManagedProcessStatus::Finished;
    }
}
