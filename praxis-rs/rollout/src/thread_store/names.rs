use std::collections::HashMap;
use std::collections::HashSet;
use std::io;
use std::path::Path;

use praxis_protocol::ThreadId;
use tracing::warn;

pub struct ThreadNameResolver<'a> {
    state_db: Option<&'a praxis_state::StateRuntime>,
}

pub struct ThreadNameWriter<'a> {
    state_db: Option<&'a praxis_state::StateRuntime>,
    praxis_home: Option<&'a Path>,
}

impl<'a> ThreadNameResolver<'a> {
    pub fn new(state_db: Option<&'a praxis_state::StateRuntime>) -> Self {
        Self { state_db }
    }

    pub async fn resolve_names(&self, thread_ids: &HashSet<ThreadId>) -> HashMap<ThreadId, String> {
        if thread_ids.is_empty() {
            return HashMap::new();
        }
        let Some(state_db) = self.state_db else {
            return HashMap::new();
        };
        match state_db.get_thread_names(thread_ids).await {
            Ok(names) => names,
            Err(err) => {
                warn!("state db get_thread_names failed: {err}");
                HashMap::new()
            }
        }
    }

    pub async fn resolve_name(&self, thread_id: ThreadId) -> Option<String> {
        let thread_ids = HashSet::from([thread_id]);
        self.resolve_names(&thread_ids).await.remove(&thread_id)
    }
}

impl<'a> ThreadNameWriter<'a> {
    pub fn new(state_db: Option<&'a praxis_state::StateRuntime>) -> Self {
        Self {
            state_db,
            praxis_home: None,
        }
    }

    pub fn with_praxis_home(
        state_db: Option<&'a praxis_state::StateRuntime>,
        praxis_home: &'a Path,
    ) -> Self {
        Self {
            state_db,
            praxis_home: Some(praxis_home),
        }
    }

    pub async fn write_name(&self, thread_id: ThreadId, name: &str) -> io::Result<()> {
        let native_exists = if let Some(praxis_home) = self.praxis_home {
            let native_id =
                praxis_thread_store_contracts::ThreadId::parse(thread_id.to_string().as_str())
                    .map_err(|error| io::Error::other(error.to_string()))?;
            let native_store =
                praxis_thread_store::ThreadStore::from_praxis_home(praxis_home.to_path_buf());
            let exists = native_store.thread_exists(native_id).await;
            if exists {
                native_store
                    .set_name(native_id, Some(name.to_owned()))
                    .await
                    .map_err(|error| io::Error::other(error.to_string()))?;
            }
            exists
        } else {
            false
        };
        match self.state_db {
            Some(state_db) => match state_db.set_thread_name(thread_id, name).await {
                Ok(()) => Ok(()),
                Err(error) if native_exists => {
                    warn!("failed to update compatibility thread name projection: {error}");
                    Ok(())
                }
                Err(error) => Err(io::Error::other(error)),
            },
            None if native_exists => Ok(()),
            None => Err(io::Error::other(
                "no persisted thread store is available for thread name write",
            )),
        }
    }
}
