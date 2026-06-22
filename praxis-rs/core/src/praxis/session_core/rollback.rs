use praxis_protocol::protocol::Event;
use praxis_protocol::protocol::EventMsg;
use praxis_protocol::protocol::PraxisErrorInfo;
use praxis_protocol::protocol::RolloutItem;
use praxis_protocol::protocol::ThreadRolledBackEvent;

use crate::rollout::RolloutRecorder;

use super::super::Session;

impl Session {
    pub(crate) async fn rollback_thread(&self, sub_id: String, num_turns: u32) {
        if num_turns == 0 {
            self.raw_event_emitter(sub_id)
                .error(
                    "num_turns must be >= 1",
                    Some(PraxisErrorInfo::ThreadRollbackFailed),
                )
                .await;
            return;
        }

        let has_active_turn = { self.active_turn.lock().await.is_some() };
        if has_active_turn {
            self.raw_event_emitter(sub_id)
                .error(
                    "Cannot rollback while a turn is in progress.",
                    Some(PraxisErrorInfo::ThreadRollbackFailed),
                )
                .await;
            return;
        }

        let turn_context = self.new_default_turn_with_sub_id(sub_id).await;
        let rollout_path = {
            let recorder = {
                let guard = self.services.rollout.lock().await;
                guard.clone()
            };
            let Some(recorder) = recorder else {
                self.raw_event_emitter(turn_context.sub_id.clone())
                    .error(
                        "thread rollback requires a persisted rollout path",
                        Some(PraxisErrorInfo::ThreadRollbackFailed),
                    )
                    .await;
                return;
            };
            recorder.rollout_path().to_path_buf()
        };
        if let Some(recorder) = {
            let guard = self.services.rollout.lock().await;
            guard.clone()
        } && let Err(err) = recorder.flush().await
        {
            self.raw_event_emitter(turn_context.sub_id.clone())
                .error(
                    format!(
                        "failed to flush rollout `{}` for rollback replay: {err}",
                        rollout_path.display()
                    ),
                    Some(PraxisErrorInfo::ThreadRollbackFailed),
                )
                .await;
            return;
        }

        let rollout_history_result =
            RolloutRecorder::get_rollout_history(rollout_path.as_path()).await;
        let initial_history = match rollout_history_result {
            Ok(history) => history,
            Err(err) => {
                self.raw_event_emitter(turn_context.sub_id.clone())
                    .error(
                        format!(
                            "failed to load rollout `{}` for rollback replay: {err}",
                            rollout_path.display()
                        ),
                        Some(PraxisErrorInfo::ThreadRollbackFailed),
                    )
                    .await;
                return;
            }
        };

        let rollback_event = ThreadRolledBackEvent { num_turns };
        let rollback_msg = EventMsg::ThreadRolledBack(rollback_event.clone());
        let replay_items = initial_history
            .get_rollout_items()
            .into_iter()
            .chain(std::iter::once(RolloutItem::EventMsg(rollback_msg.clone())))
            .collect::<Vec<_>>();
        self.persist_rollout_items(&[RolloutItem::EventMsg(rollback_msg.clone())])
            .await;
        self.flush_rollout().await;
        self.apply_rollout_reconstruction(turn_context.as_ref(), replay_items.as_slice())
            .await;
        self.recompute_token_usage(turn_context.as_ref()).await;

        self.deliver_event_raw(Event {
            id: turn_context.sub_id.clone(),
            msg: rollback_msg,
        })
        .await;
    }
}
