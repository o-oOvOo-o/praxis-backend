use std::sync::Arc;

use praxis_protocol::ThreadId;
#[cfg(test)]
use praxis_protocol::models::ResponseItem;
use praxis_protocol::protocol::Op;

use crate::error::PraxisErr;
use crate::error::Result as PraxisResult;
use crate::praxis_thread::PraxisThread;

use super::super::ThreadManagerInner;

impl ThreadManagerInner {
    pub(crate) async fn list_thread_ids(&self) -> Vec<ThreadId> {
        self.threads.list_ids().await
    }

    /// Fetch a thread by ID or return ThreadNotFound.
    pub(crate) async fn get_thread(&self, thread_id: ThreadId) -> PraxisResult<Arc<PraxisThread>> {
        self.threads
            .get(thread_id)
            .await
            .ok_or_else(|| PraxisErr::ThreadNotFound(thread_id))
    }

    /// Send an operation to a thread by ID.
    pub(crate) async fn send_op(&self, thread_id: ThreadId, op: Op) -> PraxisResult<String> {
        let thread = self.get_thread(thread_id).await?;
        if let Some(ops_log) = &self.ops_log
            && let Ok(mut log) = ops_log.lock()
        {
            log.push((thread_id, op.clone()));
        }
        thread.submit(op).await
    }

    #[cfg(test)]
    /// Append a prebuilt message to a thread by ID outside the normal user-input path.
    pub(crate) async fn append_message(
        &self,
        thread_id: ThreadId,
        message: ResponseItem,
    ) -> PraxisResult<String> {
        let thread = self.get_thread(thread_id).await?;
        thread.append_message(message).await
    }

    /// Remove a thread from the manager by ID, returning it when present.
    pub(crate) async fn remove_thread(&self, thread_id: &ThreadId) -> Option<Arc<PraxisThread>> {
        self.threads.remove(thread_id).await
    }
}
