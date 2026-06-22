use praxis_protocol::config_types::ModeKind;
use praxis_protocol::items::TurnItem;
use praxis_protocol::models::ResponseInputItem;
use praxis_protocol::models::ResponseItem;
use praxis_rollout::state_db;
use tracing::debug;

use crate::parse_turn_item;
use crate::praxis::Session;
use crate::praxis::TurnContext;
use crate::turn_assistant_text::apply_visible_assistant_text;
use crate::turn_assistant_text::last_assistant_message_from_item;
use crate::turn_assistant_text::memory_thread_ids_from_assistant_text;
use crate::turn_assistant_text::raw_assistant_output_text_from_item;
use crate::turn_image_output::save_generated_image_for_turn_item;

pub(crate) async fn record_completed_response_item(
    sess: &Session,
    turn_context: &TurnContext,
    item: &ResponseItem,
) {
    sess.record_conversation_items(turn_context, std::slice::from_ref(item))
        .await;
    maybe_mark_thread_memory_mode_polluted_from_web_search(sess, turn_context, item).await;
    record_stage1_output_usage_for_completed_item(turn_context, item).await;
}

async fn maybe_mark_thread_memory_mode_polluted_from_web_search(
    sess: &Session,
    turn_context: &TurnContext,
    item: &ResponseItem,
) {
    if !turn_context
        .config
        .memories
        .no_memories_if_mcp_or_web_search
        || !matches!(item, ResponseItem::WebSearchCall { .. })
    {
        return;
    }
    state_db::mark_thread_memory_mode_polluted(
        sess.services.state_db.as_deref(),
        sess.conversation_id,
        "record_completed_response_item",
    )
    .await;
}

async fn record_stage1_output_usage_for_completed_item(
    turn_context: &TurnContext,
    item: &ResponseItem,
) {
    let Some(raw_text) = raw_assistant_output_text_from_item(item) else {
        return;
    };

    let thread_ids = memory_thread_ids_from_assistant_text(&raw_text);
    if thread_ids.is_empty() {
        return;
    }

    if let Some(db) = state_db::get_state_db(turn_context.config.as_ref()).await {
        let _ = db.record_stage1_output_usage(&thread_ids).await;
    }
}

pub(crate) struct CompletedResponseItemSink<'a> {
    sess: &'a Session,
    turn_context: &'a TurnContext,
    plan_mode: bool,
}

impl<'a> CompletedResponseItemSink<'a> {
    pub(crate) fn new(sess: &'a Session, turn_context: &'a TurnContext) -> Self {
        Self {
            sess,
            turn_context,
            plan_mode: turn_context.collaboration_mode.mode == ModeKind::Plan,
        }
    }

    pub(crate) async fn emit_and_record(
        &self,
        item: &ResponseItem,
        previously_active_item: Option<&TurnItem>,
    ) -> Option<String> {
        if let Some(turn_item) =
            handle_non_tool_response_item(self.sess, self.turn_context, item, self.plan_mode).await
        {
            self.emit_completed_turn_item(turn_item, previously_active_item)
                .await;
        }
        self.record_completed(item).await
    }

    pub(crate) async fn record_completed(&self, item: &ResponseItem) -> Option<String> {
        record_completed_response_item(self.sess, self.turn_context, item).await;
        last_assistant_message_from_item(item, self.plan_mode)
    }

    async fn emit_completed_turn_item(
        &self,
        turn_item: TurnItem,
        previously_active_item: Option<&TurnItem>,
    ) {
        if previously_active_item.is_none() {
            let started_item = started_item_for_completed_turn_item(turn_item.clone());
            self.sess
                .emit_turn_item_started(self.turn_context, &started_item)
                .await;
        }
        self.sess
            .emit_turn_item_completed(self.turn_context, turn_item)
            .await;
    }
}

fn started_item_for_completed_turn_item(mut turn_item: TurnItem) -> TurnItem {
    if let TurnItem::ImageGeneration(item) = &mut turn_item {
        item.status = "in_progress".to_string();
        item.revised_prompt = None;
        item.result.clear();
        item.saved_path = None;
    }
    turn_item
}

pub(crate) async fn handle_non_tool_response_item(
    sess: &Session,
    turn_context: &TurnContext,
    item: &ResponseItem,
    plan_mode: bool,
) -> Option<TurnItem> {
    debug!(?item, "Output item");

    match item {
        ResponseItem::Message { .. }
        | ResponseItem::Reasoning { .. }
        | ResponseItem::WebSearchCall { .. }
        | ResponseItem::ImageGenerationCall { .. } => {
            let mut turn_item = parse_turn_item(item)?;
            if let TurnItem::AgentMessage(agent_message) = &mut turn_item {
                apply_visible_assistant_text(agent_message, plan_mode);
            }
            if let TurnItem::ImageGeneration(image_item) = &mut turn_item {
                save_generated_image_for_turn_item(sess, turn_context, image_item).await;
            }
            Some(turn_item)
        }
        ResponseItem::FunctionCallOutput { .. }
        | ResponseItem::CustomToolCallOutput { .. }
        | ResponseItem::ToolSearchOutput { .. } => {
            debug!("unexpected tool output from stream");
            None
        }
        _ => None,
    }
}

pub(crate) fn response_input_to_response_item(input: &ResponseInputItem) -> Option<ResponseItem> {
    match input {
        ResponseInputItem::FunctionCallOutput { call_id, output } => {
            Some(ResponseItem::FunctionCallOutput {
                call_id: call_id.clone(),
                output: output.clone(),
            })
        }
        ResponseInputItem::CustomToolCallOutput {
            call_id,
            name,
            output,
        } => Some(ResponseItem::CustomToolCallOutput {
            call_id: call_id.clone(),
            name: name.clone(),
            output: output.clone(),
        }),
        ResponseInputItem::McpToolCallOutput { call_id, output } => {
            let output = output.as_function_call_output_payload();
            Some(ResponseItem::FunctionCallOutput {
                call_id: call_id.clone(),
                output,
            })
        }
        ResponseInputItem::ToolSearchOutput {
            call_id,
            status,
            execution,
            tools,
        } => Some(ResponseItem::ToolSearchOutput {
            call_id: Some(call_id.clone()),
            status: status.clone(),
            execution: execution.clone(),
            tools: tools.clone(),
        }),
        _ => None,
    }
}

#[cfg(test)]
#[path = "turn_output_items_tests.rs"]
mod tests;
