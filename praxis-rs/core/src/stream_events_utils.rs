use std::path::Path;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use praxis_protocol::config_types::ModeKind;
use praxis_protocol::items::TurnItem;
use praxis_utils_stream_parser::strip_citations;
use tokio_util::sync::CancellationToken;

use crate::error::PraxisErr;
use crate::error::Result;
use crate::function_tool::FunctionCallError;
use crate::history_preview::HistoryPreview;
use crate::memories::citations::get_thread_id_from_citations;
use crate::memories::citations::parse_memory_citation;
use crate::parse_turn_item;
use crate::praxis::Session;
use crate::praxis::TurnContext;
use crate::tools::router::ToolRouter;
use crate::tools::tool_call_runtime::ToolCallRuntime;
use futures::Future;
use praxis_protocol::models::ContentItem;
use praxis_protocol::models::DeveloperInstructions;
use praxis_protocol::models::FunctionCallOutputBody;
use praxis_protocol::models::FunctionCallOutputPayload;
use praxis_protocol::models::MessagePhase;
use praxis_protocol::models::ResponseInputItem;
use praxis_protocol::models::ResponseItem;
use praxis_rollout::state_db;
use praxis_utils_stream_parser::strip_proposed_plan_blocks;
use tracing::debug;
use tracing::instrument;
use tracing::warn;

const GENERATED_IMAGE_ARTIFACTS_DIR: &str = "generated_images";

pub(crate) fn image_generation_artifact_path(
    praxis_home: &Path,
    session_id: &str,
    call_id: &str,
) -> PathBuf {
    let sanitize = |value: &str| {
        let mut sanitized: String = value
            .chars()
            .map(|ch| {
                if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                    ch
                } else {
                    '_'
                }
            })
            .collect();
        if sanitized.is_empty() {
            sanitized = "generated_image".to_string();
        }
        sanitized
    };

    praxis_home
        .join(GENERATED_IMAGE_ARTIFACTS_DIR)
        .join(sanitize(session_id))
        .join(format!("{}.png", sanitize(call_id)))
}

fn strip_hidden_assistant_markup(text: &str, plan_mode: bool) -> String {
    let (without_citations, _) = strip_citations(text);
    if plan_mode {
        strip_proposed_plan_blocks(&without_citations)
    } else {
        without_citations
    }
}

fn strip_hidden_assistant_markup_and_parse_memory_citation(
    text: &str,
    plan_mode: bool,
) -> (
    String,
    Option<praxis_protocol::memory_citation::MemoryCitation>,
) {
    let (without_citations, citations) = strip_citations(text);
    let visible_text = if plan_mode {
        strip_proposed_plan_blocks(&without_citations)
    } else {
        without_citations
    };
    (visible_text, parse_memory_citation(citations))
}

pub(crate) fn raw_assistant_output_text_from_item(item: &ResponseItem) -> Option<String> {
    if let ResponseItem::Message { role, content, .. } = item
        && role == "assistant"
    {
        let combined = content
            .iter()
            .filter_map(|ci| match ci {
                praxis_protocol::models::ContentItem::OutputText { text } => Some(text.as_str()),
                _ => None,
            })
            .collect::<String>();
        return Some(combined);
    }
    None
}

async fn save_image_generation_result(
    praxis_home: &std::path::Path,
    session_id: &str,
    call_id: &str,
    result: &str,
) -> Result<PathBuf> {
    let bytes = BASE64_STANDARD
        .decode(result.trim().as_bytes())
        .map_err(|err| {
            PraxisErr::InvalidRequest(format!("invalid image generation payload: {err}"))
        })?;
    let path = image_generation_artifact_path(praxis_home, session_id, call_id);
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    tokio::fs::write(&path, bytes).await?;
    Ok(path)
}

/// Persist a completed model response item and record any cited memory usage.
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

    let (_, citations) = strip_citations(&raw_text);
    let thread_ids = get_thread_id_from_citations(citations);
    if thread_ids.is_empty() {
        return;
    }

    if let Some(db) = state_db::get_state_db(turn_context.config.as_ref()).await {
        let _ = db.record_stage1_output_usage(&thread_ids).await;
    }
}

/// Handle a completed output item from the model stream, recording it and
/// queuing any tool execution futures. This records items immediately so
/// history and rollout stay in sync even if the turn is later cancelled.
pub(crate) type InFlightFuture<'f> =
    Pin<Box<dyn Future<Output = Result<ResponseInputItem>> + Send + 'f>>;

#[derive(Default)]
pub(crate) struct OutputItemResult {
    pub last_agent_message: Option<String>,
    pub needs_follow_up: bool,
    pub tool_future: Option<InFlightFuture<'static>>,
}

pub(crate) struct HandleOutputCtx {
    pub sess: Arc<Session>,
    pub turn_context: Arc<TurnContext>,
    pub tool_runtime: ToolCallRuntime,
    pub cancellation_token: CancellationToken,
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

#[instrument(level = "trace", skip_all)]
pub(crate) async fn handle_output_item_done(
    ctx: &mut HandleOutputCtx,
    item: ResponseItem,
    previously_active_item: Option<TurnItem>,
) -> Result<OutputItemResult> {
    let mut output = OutputItemResult::default();

    match ToolRouter::build_tool_call(ctx.sess.as_ref(), item.clone()).await {
        // The model emitted a tool call; log it, persist the item immediately, and queue the tool execution.
        Ok(Some(call)) => {
            if ctx
                .turn_context
                .tool_loop_guard
                .should_hide_tool(&call.tool_name)
            {
                warn!(
                    tool_name = call.tool_name.as_str(),
                    "hidden tool call suppressed after tool loop guard intervention"
                );
                let final_item = hidden_tool_loop_final_item(ctx, call.tool_name.as_str()).await;
                let sink =
                    CompletedResponseItemSink::new(ctx.sess.as_ref(), ctx.turn_context.as_ref());
                output.last_agent_message = sink.emit_and_record(&final_item, None).await;
                return Ok(output);
            }

            let payload_preview = call.payload.log_payload().into_owned();
            tracing::info!(
                thread_id = %ctx.sess.conversation_id,
                "ToolCall: {} {}",
                call.tool_name,
                payload_preview
            );

            record_completed_response_item(ctx.sess.as_ref(), ctx.turn_context.as_ref(), &item)
                .await;

            let cancellation_token = ctx.cancellation_token.child_token();
            let tool_future: InFlightFuture<'static> = Box::pin(
                ctx.tool_runtime
                    .clone()
                    .handle_tool_call(call, cancellation_token),
            );

            output.needs_follow_up = true;
            output.tool_future = Some(tool_future);
        }
        // No tool call: convert messages/reasoning into turn items and mark them as complete.
        Ok(None) => {
            let sink = CompletedResponseItemSink::new(ctx.sess.as_ref(), ctx.turn_context.as_ref());
            output.last_agent_message = sink
                .emit_and_record(&item, previously_active_item.as_ref())
                .await;
        }
        // Guardrail: the model issued a LocalShellCall without an id; surface the error back into history.
        Err(FunctionCallError::MissingLocalShellCallId) => {
            let msg = "LocalShellCall without call_id or id";
            ctx.turn_context
                .session_telemetry
                .log_tool_failed("local_shell", msg);
            tracing::error!(msg);

            let response = ResponseInputItem::FunctionCallOutput {
                call_id: String::new(),
                output: FunctionCallOutputPayload {
                    body: FunctionCallOutputBody::Text(msg.to_string()),
                    ..Default::default()
                },
            };
            record_completed_response_item(ctx.sess.as_ref(), ctx.turn_context.as_ref(), &item)
                .await;
            if let Some(response_item) = response_input_to_response_item(&response) {
                ctx.sess
                    .record_conversation_items(
                        &ctx.turn_context,
                        std::slice::from_ref(&response_item),
                    )
                    .await;
            }

            output.needs_follow_up = true;
        }
        // The tool request should be answered directly (or was denied); push that response into the transcript.
        Err(FunctionCallError::RespondToModel(message)) => {
            let response = ResponseInputItem::FunctionCallOutput {
                call_id: String::new(),
                output: FunctionCallOutputPayload {
                    body: FunctionCallOutputBody::Text(message),
                    ..Default::default()
                },
            };
            record_completed_response_item(ctx.sess.as_ref(), ctx.turn_context.as_ref(), &item)
                .await;
            if let Some(response_item) = response_input_to_response_item(&response) {
                ctx.sess
                    .record_conversation_items(
                        &ctx.turn_context,
                        std::slice::from_ref(&response_item),
                    )
                    .await;
            }

            output.needs_follow_up = true;
        }
        // A fatal error occurred; surface it back into history.
        Err(FunctionCallError::Fatal(message)) => {
            return Err(PraxisErr::Fatal(message));
        }
    }

    Ok(output)
}

async fn tool_loop_guard_final_item(sess: Arc<Session>, tool_name: &str) -> ResponseItem {
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

async fn hidden_tool_loop_final_item(ctx: &HandleOutputCtx, tool_name: &str) -> ResponseItem {
    tool_loop_guard_final_item(Arc::clone(&ctx.sess), tool_name).await
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
                let combined = agent_message
                    .content
                    .iter()
                    .map(|entry| match entry {
                        praxis_protocol::items::AgentMessageContent::Text { text } => text.as_str(),
                    })
                    .collect::<String>();
                let (stripped, memory_citation) =
                    strip_hidden_assistant_markup_and_parse_memory_citation(&combined, plan_mode);
                agent_message.content =
                    vec![praxis_protocol::items::AgentMessageContent::Text { text: stripped }];
                agent_message.memory_citation = memory_citation;
            }
            if let TurnItem::ImageGeneration(image_item) = &mut turn_item {
                let session_id = sess.conversation_id.to_string();
                match save_image_generation_result(
                    turn_context.config.praxis_home.as_path(),
                    &session_id,
                    &image_item.id,
                    &image_item.result,
                )
                .await
                {
                    Ok(path) => {
                        image_item.saved_path = Some(path.to_string_lossy().into_owned());
                        let image_output_path = image_generation_artifact_path(
                            turn_context.config.praxis_home.as_path(),
                            &session_id,
                            "<image_id>",
                        );
                        let image_output_dir = image_output_path
                            .parent()
                            .unwrap_or(turn_context.config.praxis_home.as_path());
                        let message: ResponseItem = DeveloperInstructions::new(format!(
                            "Generated images are saved to {} as {} by default.",
                            image_output_dir.display(),
                            image_output_path.display(),
                        ))
                        .into();
                        let copy_message: ResponseItem = DeveloperInstructions::new(
                            "If you need to use a generated image at another path, copy it and leave the original in place unless the user explicitly asks you to delete it."
                                .to_string(),
                        )
                        .into();
                        sess.record_conversation_items(turn_context, &[message, copy_message])
                            .await;
                    }
                    Err(err) => {
                        let output_path = image_generation_artifact_path(
                            turn_context.config.praxis_home.as_path(),
                            &session_id,
                            &image_item.id,
                        );
                        let output_dir = output_path
                            .parent()
                            .unwrap_or(turn_context.config.praxis_home.as_path());
                        tracing::warn!(
                            call_id = %image_item.id,
                            output_dir = %output_dir.display(),
                            "failed to save generated image: {err}"
                        );
                    }
                }
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

pub(crate) fn last_assistant_message_from_item(
    item: &ResponseItem,
    plan_mode: bool,
) -> Option<String> {
    if let Some(combined) = raw_assistant_output_text_from_item(item) {
        if combined.is_empty() {
            return None;
        }
        let stripped = strip_hidden_assistant_markup(&combined, plan_mode);
        if stripped.trim().is_empty() {
            return None;
        }
        return Some(stripped);
    }
    None
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
#[path = "stream_events_utils_tests.rs"]
mod tests;
