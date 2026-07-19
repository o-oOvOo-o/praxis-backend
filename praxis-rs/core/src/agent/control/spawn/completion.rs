use super::super::*;

impl AgentControl {
    pub(super) async fn complete_spawned_thread(
        &self,
        state: &Arc<ThreadManagerInner>,
        new_thread: &crate::thread_manager::ThreadSpawnResult,
        notification_source: Option<&SessionSource>,
        initial_operation: Option<Op>,
    ) -> PraxisResult<()> {
        state.notify_thread_created(new_thread.thread_id);

        self.persist_thread_spawn_edge_for_source(
            new_thread.thread.as_ref(),
            new_thread.thread_id,
            notification_source,
        )
        .await;

        if let Some(initial_operation) = initial_operation {
            self.submit_turn_operation(new_thread.thread_id, initial_operation)
                .await?;
        }
        Ok(())
    }
}
