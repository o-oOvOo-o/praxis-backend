use std::sync::Arc;
use std::sync::Weak;

use crate::agent::registry::AgentRegistry;
use crate::thread_manager::ThreadManagerInner;

/// Control-plane handle for multi-agent operations.
/// `AgentControl` is held by each session through `SessionServices`.
#[derive(Clone, Default)]
pub(crate) struct AgentControl {
    /// Weak handle back to the global thread registry/state.
    pub(super) manager: Weak<ThreadManagerInner>,
    pub(super) state: Arc<AgentRegistry>,
}

impl AgentControl {
    /// Construct a new `AgentControl` that can spawn/message agents via the given manager state.
    pub(crate) fn new(manager: Weak<ThreadManagerInner>) -> Self {
        Self {
            manager,
            ..Default::default()
        }
    }
}
