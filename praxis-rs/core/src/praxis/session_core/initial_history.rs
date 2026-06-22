use praxis_protocol::protocol::EventMsg;
use praxis_protocol::protocol::InitialHistory;
use praxis_protocol::protocol::RolloutItem;
use praxis_protocol::protocol::SessionSource;
use praxis_protocol::protocol::TokenUsageInfo;
use tracing::warn;

use super::super::PreviousTurnSettings;
use super::super::Session;
use super::super::TurnContext;

impl Session {
    pub(in crate::praxis) async fn record_initial_history(
        &self,
        conversation_history: InitialHistory,
    ) {
        let turn_context = self.new_default_turn().await;
        let is_subagent = {
            let state = self.state.lock().await;
            matches!(
                state.session_configuration.session_source,
                SessionSource::SubAgent(_)
            )
        };
        match conversation_history {
            InitialHistory::New => {
                // Defer initial context insertion until the first real turn starts.
                self.set_previous_turn_settings(/*previous_turn_settings*/ None)
                    .await;
            }
            InitialHistory::Resumed(resumed_history) => {
                let rollout_items = resumed_history.history;
                let previous_turn_settings = self
                    .apply_rollout_reconstruction(&turn_context, &rollout_items)
                    .await;

                let curr: &str = turn_context.model_info.slug.as_str();
                if let Some(prev) = previous_turn_settings
                    .as_ref()
                    .map(|settings| settings.model.as_str())
                    .filter(|model| *model != curr)
                {
                    warn!("resuming session with different model: previous={prev}, current={curr}");
                    self.turn_event_emitter(&turn_context)
                        .warning(format!(
                            "This session was recorded with model `{prev}` but is resuming with `{curr}`. \
                         Consider switching back to `{prev}` as it may affect Praxis performance."
                        ))
                        .await;
                }

                if let Some(info) = Self::last_token_info_from_rollout(&rollout_items) {
                    let mut state = self.state.lock().await;
                    state.set_token_info(Some(info));
                }

                if !is_subagent {
                    self.flush_rollout().await;
                }
            }
            InitialHistory::Forked(rollout_items) => {
                self.apply_rollout_reconstruction(&turn_context, &rollout_items)
                    .await;

                if let Some(info) = Self::last_token_info_from_rollout(&rollout_items) {
                    let mut state = self.state.lock().await;
                    state.set_token_info(Some(info));
                }

                if !rollout_items.is_empty() {
                    self.persist_rollout_items(&rollout_items).await;
                }

                self.ensure_rollout_materialized().await;

                if !is_subagent {
                    self.flush_rollout().await;
                }
            }
        }
    }

    pub(super) async fn apply_rollout_reconstruction(
        &self,
        turn_context: &TurnContext,
        rollout_items: &[RolloutItem],
    ) -> Option<PreviousTurnSettings> {
        let reconstructed_rollout = self
            .reconstruct_history_from_rollout(turn_context, rollout_items)
            .await;
        let previous_turn_settings = reconstructed_rollout.previous_turn_settings.clone();
        self.replace_history(
            reconstructed_rollout.history,
            reconstructed_rollout.reference_context_item,
        )
        .await;
        self.set_previous_turn_settings(previous_turn_settings.clone())
            .await;
        previous_turn_settings
    }

    pub(super) fn last_token_info_from_rollout(
        rollout_items: &[RolloutItem],
    ) -> Option<TokenUsageInfo> {
        rollout_items.iter().rev().find_map(|item| match item {
            RolloutItem::EventMsg(EventMsg::TokenCount(ev)) => ev.info.clone(),
            _ => None,
        })
    }
}
