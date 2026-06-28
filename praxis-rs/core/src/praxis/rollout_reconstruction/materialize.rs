use praxis_protocol::models::ResponseItem;
use praxis_protocol::protocol::EventMsg;
use praxis_protocol::protocol::RolloutItem;

use crate::context_manager::ContextManager;
use crate::praxis::TurnContext;

use super::types::MaterializedHistory;

pub(super) fn materialize_history_from_replay(
    turn_context: &TurnContext,
    base_replacement_history: Option<&[ResponseItem]>,
    rollout_suffix: &[RolloutItem],
) -> MaterializedHistory {
    let mut history = ContextManager::new();
    if let Some(base_replacement_history) = base_replacement_history {
        history.replace(base_replacement_history.to_vec());
    }

    for item in rollout_suffix {
        match item {
            RolloutItem::ResponseItem(response_item) => {
                history.record_items(
                    std::iter::once(response_item),
                    turn_context.truncation_policy,
                );
            }
            RolloutItem::Compacted(compacted) => {
                if let Some(replacement_history) = &compacted.replacement_history {
                    history.replace(replacement_history.clone());
                }
            }
            RolloutItem::EventMsg(EventMsg::ThreadRolledBack(rollback)) => {
                history.drop_last_n_user_turns(rollback.num_turns);
            }
            RolloutItem::EventMsg(_)
            | RolloutItem::TurnContext(_)
            | RolloutItem::SessionMeta(_) => {}
        }
    }

    MaterializedHistory {
        history: history.raw_items().to_vec(),
    }
}
