use super::*;

impl AgentOs {
    pub(crate) async fn cleanup_thread_resources_after_abort(
        &self,
        thread_id: ThreadId,
        reason: impl Into<String>,
    ) {
        let reason = reason.into();
        let live_commands = self.live_commands_for_abort(thread_id).await;

        self.cleanup_abort_processes(thread_id, &reason, &live_commands)
            .await;
        self.finish_abort_commands(thread_id, &reason, &live_commands)
            .await;

        let cleanup = self.collect_abort_cleanup_snapshot(thread_id).await;
        self.release_abort_leftovers(&reason, &cleanup).await;

        if !live_commands.is_empty()
            || !cleanup.tickets.is_empty()
            || !cleanup.stray_lease_ids.is_empty()
        {
            self.record_event(
                "thread_resources_cleaned_after_abort",
                Some(thread_id),
                None,
                None,
                json!({
                    "reason": reason,
                    "commands": live_commands
                        .iter()
                        .map(|(command_id, _, _, _)| command_id)
                        .collect::<Vec<_>>(),
                    "tickets": cleanup
                        .tickets
                        .iter()
                        .map(|ticket| &ticket.ticket_id)
                        .collect::<Vec<_>>(),
                    "stray_leases": cleanup.stray_lease_ids,
                }),
            )
            .await;
        }
    }
}
