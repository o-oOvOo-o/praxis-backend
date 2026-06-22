use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;

use praxis_rollout::StateDbHandle;
use tokio::sync::RwLock;
use tokio::sync::watch;

use super::process::AgentOsProcessCleaner;
use super::state::AgentOsState;

pub(crate) struct AgentOs {
    pub(super) state: RwLock<AgentOsState>,
    pub(super) state_db: RwLock<Option<StateDbHandle>>,
    // Cleaners are indexed by runtime kind and owner so lease expiry can route cleanup directly.
    pub(super) process_cleaners: RwLock<HashMap<String, Vec<Arc<dyn AgentOsProcessCleaner>>>>,
    pub(super) process_cleaners_by_owner: RwLock<HashMap<String, Arc<dyn AgentOsProcessCleaner>>>,
    pub(super) lease_janitor_started: AtomicBool,
    pub(super) change_seq: AtomicU64,
    pub(super) change_tx: watch::Sender<u64>,
}

impl Default for AgentOs {
    fn default() -> Self {
        let (change_tx, _) = watch::channel(0);
        Self {
            state: RwLock::new(AgentOsState::default()),
            state_db: RwLock::new(None),
            process_cleaners: RwLock::new(HashMap::new()),
            process_cleaners_by_owner: RwLock::new(HashMap::new()),
            lease_janitor_started: AtomicBool::new(false),
            change_seq: AtomicU64::new(0),
            change_tx,
        }
    }
}

impl AgentOs {
    pub(crate) fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub(crate) fn subscribe_changes(&self) -> watch::Receiver<u64> {
        self.change_tx.subscribe()
    }

    pub(crate) fn change_sequence(&self) -> u64 {
        self.change_seq.load(Ordering::SeqCst)
    }

    pub(crate) async fn attach_state_db(&self, state_db: Option<StateDbHandle>) {
        if let Some(state_db) = state_db {
            *self.state_db.write().await = Some(state_db);
        }
    }
}
