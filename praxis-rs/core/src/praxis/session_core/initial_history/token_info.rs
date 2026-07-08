use praxis_protocol::protocol::EventMsg;
use praxis_protocol::protocol::RolloutItem;
use praxis_protocol::protocol::TokenUsageInfo;

use crate::praxis::Session;

pub(super) async fn restore_last_token_info_from_rollout(
    session: &Session,
    rollout_items: &[RolloutItem],
) {
    if let Some(info) = last_token_info_from_rollout(rollout_items) {
        {
            let mut state = session.state.lock().await;
            state.set_token_info(Some(info.clone()));
        }
        session
            .token_ledger
            .write()
            .await
            .set_token_info(Some(info));
    }
}

fn last_token_info_from_rollout(rollout_items: &[RolloutItem]) -> Option<TokenUsageInfo> {
    rollout_items.iter().rev().find_map(|item| match item {
        RolloutItem::EventMsg(EventMsg::TokenCount(ev)) => ev.info.clone(),
        _ => None,
    })
}
