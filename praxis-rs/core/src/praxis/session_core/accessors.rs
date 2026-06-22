use std::path::PathBuf;
use std::sync::Arc;

use async_channel::Sender;
use praxis_protocol::models::BaseInstructions;
use praxis_protocol::protocol::Event;
use praxis_rollout::state_db;
use tokio::sync::watch;

use crate::config::Config;
use crate::llm::runtime::LlmRuntimeCatalog;

use super::super::Session;

impl Session {
    pub(crate) async fn praxis_home(&self) -> PathBuf {
        let state = self.state.lock().await;
        state.session_configuration.praxis_home().clone()
    }

    pub(crate) fn subscribe_out_of_band_elicitation_pause_state(&self) -> watch::Receiver<bool> {
        self.out_of_band_elicitation_paused.subscribe()
    }

    pub(crate) fn set_out_of_band_elicitation_pause_state(&self, paused: bool) {
        self.out_of_band_elicitation_paused.send_replace(paused);
    }

    pub(crate) fn get_tx_event(&self) -> Sender<Event> {
        self.tx_event.clone()
    }

    pub(crate) fn state_db(&self) -> Option<state_db::StateDbHandle> {
        self.services.state_db.clone()
    }

    pub(crate) async fn original_config(&self) -> Arc<Config> {
        let state = self.state.lock().await;
        Arc::clone(&state.session_configuration.original_config_do_not_use)
    }

    pub(crate) fn next_internal_sub_id(&self) -> String {
        let id = self
            .next_internal_sub_id
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        format!("auto-{id}")
    }

    pub(crate) fn llm_runtime_catalog(&self) -> &LlmRuntimeCatalog {
        &self.llm_runtime_catalog
    }

    pub(crate) async fn get_base_instructions(&self) -> BaseInstructions {
        let state = self.state.lock().await;
        BaseInstructions {
            text: state.session_configuration.base_instructions.clone(),
        }
    }
}
