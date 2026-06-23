mod forked_history;
mod reconstruction;
mod resumed_history;
mod session_source;
mod token_info;

use praxis_protocol::protocol::InitialHistory;

use super::super::Session;
use session_source::is_subagent_session;

impl Session {
    pub(in crate::praxis) async fn record_initial_history(
        &self,
        conversation_history: InitialHistory,
    ) {
        let turn_context = self.new_default_turn().await;
        let is_subagent = is_subagent_session(self).await;
        match conversation_history {
            InitialHistory::New => {
                // Defer initial context insertion until the first real turn starts.
                self.set_previous_turn_settings(/*previous_turn_settings*/ None)
                    .await;
            }
            InitialHistory::Resumed(resumed_history) => {
                resumed_history::record(self, &turn_context, resumed_history.history, is_subagent)
                    .await;
            }
            InitialHistory::Forked(rollout_items) => {
                forked_history::record(self, &turn_context, rollout_items, is_subagent).await;
            }
        }
    }
}
