use crate::exec::ExecToolCallOutput;
use crate::praxis::Session;
use crate::praxis::TurnContext;
use crate::user_shell_command::user_shell_command_record_item;
use praxis_protocol::models::ResponseInputItem;
use praxis_protocol::models::ResponseItem;

use super::types::UserShellCommandMode;

pub(super) async fn persist_user_shell_output(
    session: &Session,
    turn_context: &TurnContext,
    raw_command: &str,
    exec_output: &ExecToolCallOutput,
    mode: UserShellCommandMode,
) {
    let output_item = user_shell_command_record_item(raw_command, exec_output, turn_context);

    if mode == UserShellCommandMode::StandaloneTurn {
        session
            .record_conversation_items(turn_context, std::slice::from_ref(&output_item))
            .await;
        session.ensure_rollout_materialized().await;
        return;
    }

    let response_input_item = match output_item {
        ResponseItem::Message { role, content, .. } => ResponseInputItem::Message { role, content },
        _ => unreachable!("user shell command output record should always be a message"),
    };

    if let Err(items) = session
        .inject_response_items(vec![response_input_item])
        .await
    {
        let response_items = items
            .into_iter()
            .map(ResponseItem::from)
            .collect::<Vec<_>>();
        session
            .record_conversation_items(turn_context, &response_items)
            .await;
    }
}
