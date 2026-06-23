mod completion;
mod replay_plan;
mod rollout_source;
mod validation;

use praxis_protocol::protocol::ThreadRolledBackEvent;

use super::super::Session;

impl Session {
    pub(crate) async fn rollback_thread(&self, sub_id: String, num_turns: u32) {
        if validation::reject_invalid_request(self, &sub_id, num_turns).await {
            return;
        }

        let turn_context = self.new_default_turn_with_sub_id(sub_id).await;
        let Some(rollout_history) = rollout_source::load_flushed_history(self, &turn_context).await
        else {
            return;
        };

        let rollback_event = ThreadRolledBackEvent { num_turns };
        let rollback_msg = replay_plan::rollback_message(rollback_event);
        let replay_items = replay_plan::build_items(rollout_history, rollback_msg.clone());
        completion::commit(self, turn_context.as_ref(), rollback_msg, replay_items).await;
    }
}
