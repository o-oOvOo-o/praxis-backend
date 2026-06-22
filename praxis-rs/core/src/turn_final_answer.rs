use std::sync::Arc;

use praxis_protocol::models::ContentItem;
use praxis_protocol::models::MessagePhase;
use praxis_protocol::models::ResponseItem;

use crate::history_preview::HistoryPreview;
use crate::praxis::Session;
use crate::praxis::TurnContext;
use crate::turn_output_items::CompletedResponseItemSink;

pub(crate) async fn tool_loop_guard_final_item(
    sess: Arc<Session>,
    tool_name: &str,
) -> ResponseItem {
    final_answer_item(
        sess,
        format!(
            "Tool loop stopped: `{tool_name}` was no longer available after AgentOS reported no live sub-agents or pending work."
        ),
        true,
    )
    .await
}

async fn subagent_workflow_empty_final_item(sess: Arc<Session>) -> ResponseItem {
    final_answer_item(
        sess,
        "Sub-agent workflow completed, but the model ended the turn without a final assistant message.".to_string(),
        true,
    )
    .await
}

async fn subagent_workflow_incomplete_final_item(sess: Arc<Session>) -> ResponseItem {
    final_answer_item(
        sess,
        "Sub-agent workflow ended without a final assistant message while live sub-agents still remain. Not emitting the requested completion marker.".to_string(),
        false,
    )
    .await
}

async fn model_empty_final_item(sess: Arc<Session>) -> ResponseItem {
    final_answer_item(
        sess,
        "Model completed the turn without a final assistant message. Not emitting any requested completion marker.".to_string(),
        false,
    )
    .await
}

async fn has_live_subagents(sess: &Arc<Session>, turn_context: &Arc<TurnContext>) -> bool {
    sess.services
        .agent_control
        .list_agents(sess.conversation_id, &turn_context.session_source, None)
        .await
        .map(|agents| agents.into_iter().any(|agent| agent.agent_name != "/root"))
        .unwrap_or(true)
}

pub(crate) async fn synthetic_final_item_for_guard(
    sess: Arc<Session>,
    turn_context: &Arc<TurnContext>,
    include_model_empty: bool,
) -> Option<ResponseItem> {
    let guard = &turn_context.tool_loop_guard;
    if guard.has_terminal_list_agents() {
        Some(tool_loop_guard_final_item(sess, "list_agents").await)
    } else if guard.has_subagent_tool_calls() {
        Some(if has_live_subagents(&sess, turn_context).await {
            subagent_workflow_incomplete_final_item(Arc::clone(&sess)).await
        } else {
            subagent_workflow_empty_final_item(Arc::clone(&sess)).await
        })
    } else if include_model_empty {
        Some(model_empty_final_item(sess).await)
    } else {
        None
    }
}

pub(crate) async fn emit_synthetic_final_answer(
    sess: &Arc<Session>,
    turn_context: &Arc<TurnContext>,
    final_item: ResponseItem,
) -> Option<String> {
    let sink = CompletedResponseItemSink::new(sess.as_ref(), turn_context.as_ref());
    sink.emit_and_record(&final_item, None).await
}

async fn final_answer_item(
    sess: Arc<Session>,
    mut text: String,
    include_requested_marker: bool,
) -> ResponseItem {
    if include_requested_marker {
        let marker = HistoryPreview::for_session(sess.as_ref())
            .await
            .requested_final_line_marker();
        if let Some(marker) = marker {
            text.push('\n');
            text.push_str(marker.as_str());
        }
    }
    ResponseItem::Message {
        id: None,
        role: "assistant".to_string(),
        content: vec![ContentItem::OutputText { text }],
        end_turn: Some(true),
        phase: Some(MessagePhase::FinalAnswer),
    }
}
