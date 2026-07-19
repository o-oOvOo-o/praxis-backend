use std::collections::HashMap;
use std::collections::HashSet;
use std::sync::Arc;
use tokio::runtime::Handle;
use tokio::sync::RwLock;

use praxis_protocol::ThreadId;

use crate::praxis_thread::PraxisThread;

#[derive(Default)]
pub(super) struct ThreadRegistry {
    state: Arc<RwLock<ThreadRegistryState>>,
}

#[derive(Default)]
struct ThreadRegistryState {
    threads: HashMap<ThreadId, Arc<PraxisThread>>,
    reserved_ids: HashSet<ThreadId>,
}

pub(super) struct ThreadIdReservation {
    state: Arc<RwLock<ThreadRegistryState>>,
    thread_id: ThreadId,
    runtime: Handle,
}

impl Drop for ThreadIdReservation {
    fn drop(&mut self) {
        if let Ok(mut state) = self.state.try_write() {
            state.reserved_ids.remove(&self.thread_id);
            return;
        }
        let state = Arc::clone(&self.state);
        let thread_id = self.thread_id;
        self.runtime.spawn(async move {
            state.write().await.reserved_ids.remove(&thread_id);
        });
    }
}

impl ThreadRegistry {
    pub(super) async fn list_ids(&self) -> Vec<ThreadId> {
        self.state.read().await.threads.keys().copied().collect()
    }

    pub(super) async fn snapshot_threads(&self) -> Vec<Arc<PraxisThread>> {
        self.state.read().await.threads.values().cloned().collect()
    }

    pub(super) async fn snapshot_entries(&self) -> Vec<(ThreadId, Arc<PraxisThread>)> {
        self.state
            .read()
            .await
            .threads
            .iter()
            .map(|(thread_id, thread)| (*thread_id, Arc::clone(thread)))
            .collect()
    }

    pub(super) async fn get(&self, thread_id: ThreadId) -> Option<Arc<PraxisThread>> {
        self.state.read().await.threads.get(&thread_id).cloned()
    }

    pub(super) async fn reserve(&self, thread_id: ThreadId) -> Option<ThreadIdReservation> {
        let mut state = self.state.write().await;
        if state.threads.contains_key(&thread_id) || state.reserved_ids.contains(&thread_id) {
            return None;
        }
        state.reserved_ids.insert(thread_id);
        Some(ThreadIdReservation {
            state: Arc::clone(&self.state),
            thread_id,
            runtime: Handle::current(),
        })
    }

    pub(super) async fn insert(
        &self,
        thread_id: ThreadId,
        thread: Arc<PraxisThread>,
        consume_reservation: bool,
    ) -> bool {
        let mut state = self.state.write().await;
        if state.threads.contains_key(&thread_id)
            || (!consume_reservation && state.reserved_ids.contains(&thread_id))
        {
            return false;
        }
        if consume_reservation && !state.reserved_ids.remove(&thread_id) {
            return false;
        }
        state.threads.insert(thread_id, thread);
        true
    }

    pub(super) async fn remove(&self, thread_id: &ThreadId) -> Option<Arc<PraxisThread>> {
        self.state.write().await.threads.remove(thread_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn dropping_contended_reservation_outside_runtime_releases_id() {
        let registry = ThreadRegistry::default();
        let thread_id = ThreadId::new();
        let reservation = registry.reserve(thread_id).await.expect("reserve id");
        let state_guard = registry.state.write().await;

        std::thread::spawn(move || drop(reservation))
            .join()
            .expect("dropping outside the runtime must not panic");
        drop(state_guard);

        tokio::time::timeout(std::time::Duration::from_secs(1), async {
            loop {
                if !registry
                    .state
                    .read()
                    .await
                    .reserved_ids
                    .contains(&thread_id)
                {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("reservation cleanup should finish");
        let replacement = registry
            .reserve(thread_id)
            .await
            .expect("reservation cleanup should release id");
        drop(replacement);
    }
}
