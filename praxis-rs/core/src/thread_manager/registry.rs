use std::collections::HashMap;
use std::collections::HashSet;
use std::sync::Arc;
use std::sync::RwLock;
use std::sync::RwLockReadGuard;
use std::sync::RwLockWriteGuard;

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

/// Exclusive right to register one thread identity.
pub(super) struct ThreadIdReservation {
    state: Arc<RwLock<ThreadRegistryState>>,
    thread_id: Option<ThreadId>,
}

impl Drop for ThreadIdReservation {
    fn drop(&mut self) {
        if let Some(thread_id) = self.thread_id.take() {
            write(&self.state).reserved_ids.remove(&thread_id);
        }
    }
}

impl ThreadRegistry {
    pub(super) fn list_ids(&self) -> Vec<ThreadId> {
        read(&self.state).threads.keys().copied().collect()
    }

    pub(super) fn snapshot_threads(&self) -> Vec<Arc<PraxisThread>> {
        read(&self.state).threads.values().cloned().collect()
    }

    pub(super) fn snapshot_entries(&self) -> Vec<(ThreadId, Arc<PraxisThread>)> {
        read(&self.state)
            .threads
            .iter()
            .map(|(thread_id, thread)| (*thread_id, Arc::clone(thread)))
            .collect()
    }

    pub(super) fn get(&self, thread_id: ThreadId) -> Option<Arc<PraxisThread>> {
        read(&self.state).threads.get(&thread_id).cloned()
    }

    pub(super) fn reserve(&self, thread_id: ThreadId) -> Option<ThreadIdReservation> {
        let mut state = write(&self.state);
        if state.threads.contains_key(&thread_id) || !state.reserved_ids.insert(thread_id) {
            return None;
        }
        Some(ThreadIdReservation {
            state: Arc::clone(&self.state),
            thread_id: Some(thread_id),
        })
    }

    pub(super) fn insert(&self, thread_id: ThreadId, thread: Arc<PraxisThread>) -> bool {
        let mut state = write(&self.state);
        if state.threads.contains_key(&thread_id) || state.reserved_ids.contains(&thread_id) {
            return false;
        }
        state.threads.insert(thread_id, thread);
        true
    }

    pub(super) fn insert_reserved(
        &self,
        mut reservation: ThreadIdReservation,
        thread: Arc<PraxisThread>,
    ) -> Option<ThreadId> {
        if !Arc::ptr_eq(&self.state, &reservation.state) {
            return None;
        }
        let thread_id = reservation.thread_id.take()?;
        let mut state = write(&self.state);
        if !state.reserved_ids.remove(&thread_id) || state.threads.contains_key(&thread_id) {
            return None;
        }
        state.threads.insert(thread_id, thread);
        Some(thread_id)
    }

    pub(super) fn remove(&self, thread_id: &ThreadId) -> Option<Arc<PraxisThread>> {
        write(&self.state).threads.remove(thread_id)
    }
}

fn read(lock: &RwLock<ThreadRegistryState>) -> RwLockReadGuard<'_, ThreadRegistryState> {
    lock.read()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn write(lock: &RwLock<ThreadRegistryState>) -> RwLockWriteGuard<'_, ThreadRegistryState> {
    lock.write()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reservation_excludes_duplicate_identity() {
        let registry = ThreadRegistry::default();
        let thread_id = ThreadId::new();
        let reservation = registry.reserve(thread_id).expect("reserve id");

        assert!(registry.reserve(thread_id).is_none());
        drop(reservation);
        assert!(registry.reserve(thread_id).is_some());
    }

    #[test]
    fn reservation_drop_releases_identity_without_runtime_cleanup() {
        let registry = Arc::new(ThreadRegistry::default());
        let thread_id = ThreadId::new();
        let reservation = registry.reserve(thread_id).expect("reserve id");

        std::thread::spawn(move || drop(reservation))
            .join()
            .expect("drop reservation");

        assert!(registry.reserve(thread_id).is_some());
    }
}
