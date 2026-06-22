use std::collections::HashMap;
use std::sync::Arc;

use praxis_protocol::ThreadId;
use tokio::sync::RwLock;

use crate::praxis_thread::PraxisThread;

#[derive(Default)]
pub(super) struct ThreadRegistry {
    threads: RwLock<HashMap<ThreadId, Arc<PraxisThread>>>,
}

impl ThreadRegistry {
    pub(super) async fn list_ids(&self) -> Vec<ThreadId> {
        self.threads.read().await.keys().copied().collect()
    }

    pub(super) async fn snapshot_threads(&self) -> Vec<Arc<PraxisThread>> {
        self.threads.read().await.values().cloned().collect()
    }

    pub(super) async fn snapshot_entries(&self) -> Vec<(ThreadId, Arc<PraxisThread>)> {
        self.threads
            .read()
            .await
            .iter()
            .map(|(thread_id, thread)| (*thread_id, Arc::clone(thread)))
            .collect()
    }

    pub(super) async fn get(&self, thread_id: ThreadId) -> Option<Arc<PraxisThread>> {
        self.threads.read().await.get(&thread_id).cloned()
    }

    pub(super) async fn insert(&self, thread_id: ThreadId, thread: Arc<PraxisThread>) {
        self.threads.write().await.insert(thread_id, thread);
    }

    pub(super) async fn remove(&self, thread_id: &ThreadId) -> Option<Arc<PraxisThread>> {
        self.threads.write().await.remove(thread_id)
    }
}
