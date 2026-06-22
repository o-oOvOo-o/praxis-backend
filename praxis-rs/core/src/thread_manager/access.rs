use std::sync::Arc;

use praxis_login::AuthManager;
use praxis_protocol::ThreadId;
use praxis_protocol::config_types::CollaborationModeMask;
use praxis_protocol::openai_models::ModelPreset;
#[cfg(test)]
use praxis_protocol::protocol::Op;
use praxis_protocol::protocol::SessionSource;
use tokio::sync::broadcast;

use crate::SkillsManager;
use crate::error::Result as PraxisResult;
use crate::mcp::McpManager;
use crate::models_manager::manager::ModelsManager;
use crate::models_manager::manager::RefreshStrategy;
use crate::plugins::PluginsManager;
use crate::praxis_thread::PraxisThread;

use super::ThreadManager;

impl ThreadManager {
    pub fn session_source(&self) -> SessionSource {
        self.state.session_source.clone()
    }

    pub fn auth_manager(&self) -> Arc<AuthManager> {
        self.state.auth_manager.clone()
    }

    pub fn skills_manager(&self) -> Arc<SkillsManager> {
        self.state.skills_manager.clone()
    }

    pub fn plugins_manager(&self) -> Arc<PluginsManager> {
        self.state.plugins_manager.clone()
    }

    pub fn mcp_manager(&self) -> Arc<McpManager> {
        self.state.mcp_manager.clone()
    }

    pub fn get_models_manager(&self) -> Arc<ModelsManager> {
        self.state.models_manager.clone()
    }

    pub async fn list_models(&self, refresh_strategy: RefreshStrategy) -> Vec<ModelPreset> {
        self.state
            .models_manager
            .list_models(refresh_strategy)
            .await
    }

    pub fn list_collaboration_modes(&self) -> Vec<CollaborationModeMask> {
        self.state.models_manager.list_collaboration_modes()
    }

    pub async fn list_thread_ids(&self) -> Vec<ThreadId> {
        self.state.list_thread_ids().await
    }

    pub fn subscribe_thread_created(&self) -> broadcast::Receiver<ThreadId> {
        self.state.thread_created_tx.subscribe()
    }

    pub async fn get_thread(&self, thread_id: ThreadId) -> PraxisResult<Arc<PraxisThread>> {
        self.state.get_thread(thread_id).await
    }

    /// Removes the thread from the manager's internal map, though the thread is stored
    /// as `Arc<PraxisThread>`, it is possible that other references to it exist elsewhere.
    /// Returns the thread if the thread was found and removed.
    pub async fn remove_thread(&self, thread_id: &ThreadId) -> Option<Arc<PraxisThread>> {
        self.state.threads.remove(thread_id).await
    }

    #[cfg(test)]
    pub(crate) fn captured_ops(&self) -> Vec<(ThreadId, Op)> {
        self.state
            .ops_log
            .as_ref()
            .and_then(|ops_log| ops_log.lock().ok().map(|log| log.clone()))
            .unwrap_or_default()
    }
}
