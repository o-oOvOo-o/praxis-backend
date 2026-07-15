use std::sync::Arc;

use praxis_protocol::items::UserMessageItem;
use praxis_protocol::models::ResponseInputItem;
use praxis_protocol::models::ResponseItem;
use praxis_protocol::user_input::UserInput;

use crate::hook_runtime::record_additional_contexts;
use crate::hook_runtime::run_user_prompt_submit_hooks;

use super::super::super::Session;
use super::super::super::TurnContext;

pub(super) async fn record_user_input_and_collect_additional_contexts(
    sess: &Arc<Session>,
    turn_context: &Arc<TurnContext>,
    input: &[UserInput],
) -> Option<Vec<String>> {
    if input.is_empty() {
        return Some(Vec::new());
    }

    let initial_input_for_turn: ResponseInputItem = ResponseInputItem::from(input.to_vec());
    let response_item: ResponseItem = initial_input_for_turn.clone().into();
    let user_prompt_submit_outcome =
        run_user_prompt_submit_hooks(sess, turn_context, UserMessageItem::new(input).message())
            .await;
    if user_prompt_submit_outcome.should_stop {
        record_additional_contexts(
            sess,
            turn_context,
            user_prompt_submit_outcome.additional_contexts,
        )
        .await;
        return None;
    }

    sess.record_user_prompt_and_emit_turn_item(turn_context.as_ref(), input, &response_item)
        .await;
    Some(user_prompt_submit_outcome.additional_contexts)
}
