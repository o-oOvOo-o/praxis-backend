use super::*;

pub(super) type LiveCommandCleanupRef = (String, String, Option<i32>, Option<String>);

pub(super) struct AbortCleanupSnapshot {
    pub(super) tickets: Vec<ExecutionTicket>,
    pub(super) stray_lease_ids: Vec<String>,
    pub(super) thread_snapshot: Option<ThreadRegistryEntry>,
}

impl AgentOs {
    pub(super) async fn live_commands_for_abort(
        &self,
        thread_id: ThreadId,
    ) -> Vec<LiveCommandCleanupRef> {
        let state = self.state.read().await;
        state
            .commands
            .values()
            .filter(|command| command.thread_id == thread_id && command.ended_at.is_none())
            .map(|command| {
                (
                    command.command_id.clone(),
                    command.task_id.clone(),
                    command.process_id,
                    command.runtime_owner_id.clone(),
                )
            })
            .collect::<Vec<_>>()
    }

    pub(super) async fn collect_abort_cleanup_snapshot(
        &self,
        thread_id: ThreadId,
    ) -> AbortCleanupSnapshot {
        let mut state = self.state.write().await;
        let now = Utc::now();
        let command_ticket_ids = state
            .commands
            .values()
            .filter(|command| command.thread_id == thread_id)
            .map(|command| command.ticket_id.clone())
            .collect::<HashSet<_>>();
        let ticket_ids = state
            .tickets
            .iter()
            .filter(|(_, ticket)| {
                ticket.thread_id == thread_id
                    && !command_ticket_ids.contains(ticket.ticket_id.as_str())
            })
            .map(|(ticket_id, _)| ticket_id.clone())
            .collect::<Vec<_>>();
        let tickets = ticket_ids
            .into_iter()
            .filter_map(|ticket_id| state.tickets.remove(ticket_id.as_str()))
            .collect::<Vec<_>>();
        let stray_lease_ids = state
            .leases
            .iter()
            .filter(|(_, lease)| lease.owner_thread_id == thread_id)
            .map(|(lease_id, _)| lease_id.clone())
            .collect::<Vec<_>>();
        let thread_snapshot = state.threads.get_mut(&thread_id).map(|thread| {
            thread.current_command_id = None;
            if matches!(
                thread.state,
                ThreadRuntimeState::Running
                    | ThreadRuntimeState::WaitingForLease
                    | ThreadRuntimeState::Stopping
            ) {
                thread.state = ThreadRuntimeState::Idle;
            }
            thread.heartbeat_at = now;
            thread.clone()
        });
        AbortCleanupSnapshot {
            tickets,
            stray_lease_ids,
            thread_snapshot,
        }
    }
}
