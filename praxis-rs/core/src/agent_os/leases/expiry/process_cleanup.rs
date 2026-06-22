use super::super::*;

impl AgentOs {
    pub(super) async fn cleanup_expired_lease_processes(
        &self,
        cleanup_processes: HashSet<(i32, Option<String>)>,
    ) {
        for (process_id, runtime_owner_id) in cleanup_processes {
            self.mark_process_status(
                process_id,
                runtime_owner_id.as_deref(),
                ManagedProcessStatus::Cleaning,
            )
            .await;
            let cleaned = self
                .cleanup_process(process_id, runtime_owner_id.as_deref())
                .await;
            if cleaned {
                self.mark_process_finished(process_id, runtime_owner_id.as_deref())
                    .await;
            }
            self.record_event(
                "lease_process_cleanup",
                None,
                None,
                None,
                json!({
                    "process_id": process_id,
                    "runtime_owner_id": runtime_owner_id,
                    "cleaned": cleaned,
                }),
            )
            .await;
        }
    }
}
