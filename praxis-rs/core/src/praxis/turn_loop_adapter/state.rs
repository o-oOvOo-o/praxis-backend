use std::collections::HashSet;

use praxis_loop::outcome::TurnCompletionMessage;
use tokio::sync::Mutex;

use super::prepare_phase::TurnPrepareOutcome;

#[derive(Debug)]
pub(super) struct PraxisTurnBridgeState {
    explicitly_enabled_connectors: Mutex<HashSet<String>>,
    model_request_input_messages: Mutex<Vec<String>>,
    stop_hook_active: Mutex<bool>,
    last_agent_message: Mutex<Option<String>>,
}

impl PraxisTurnBridgeState {
    pub(super) fn new(model_request_input_messages: Vec<String>) -> Self {
        Self {
            explicitly_enabled_connectors: Mutex::new(HashSet::new()),
            model_request_input_messages: Mutex::new(model_request_input_messages),
            stop_hook_active: Mutex::new(false),
            last_agent_message: Mutex::new(None),
        }
    }

    pub(super) async fn apply_prepare_outcome(&self, outcome: TurnPrepareOutcome) {
        self.set_explicitly_enabled_connectors(outcome.explicitly_enabled_connectors)
            .await;
    }

    async fn set_explicitly_enabled_connectors(&self, connectors: HashSet<String>) {
        *self.explicitly_enabled_connectors.lock().await = connectors;
    }

    pub(super) async fn explicitly_enabled_connectors(&self) -> HashSet<String> {
        self.explicitly_enabled_connectors.lock().await.clone()
    }

    pub(super) async fn set_model_request_input_messages(&self, messages: Vec<String>) {
        *self.model_request_input_messages.lock().await = messages;
    }

    pub(super) async fn model_request_input_messages(&self) -> Vec<String> {
        self.model_request_input_messages.lock().await.clone()
    }

    pub(super) async fn record_agent_message(&self, message: impl Into<String>) {
        self.set_last_agent_message(Some(message.into())).await;
    }

    pub(super) async fn record_completion_message(&self, message: &TurnCompletionMessage) {
        self.set_last_agent_message(message.clone().into_option())
            .await;
    }

    pub(super) async fn record_optional_agent_message(&self, message: Option<String>) {
        self.set_last_agent_message(message).await;
    }

    pub(super) async fn last_agent_message(&self) -> Option<String> {
        self.last_agent_message.lock().await.clone()
    }

    pub(super) async fn stop_hook_active(&self) -> bool {
        *self.stop_hook_active.lock().await
    }

    pub(super) async fn set_stop_hook_active(&self, active: bool) {
        *self.stop_hook_active.lock().await = active;
    }

    async fn set_last_agent_message(&self, message: Option<String>) {
        *self.last_agent_message.lock().await = message;
    }
}
