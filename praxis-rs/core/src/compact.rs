use std::sync::Arc;

use crate::Prompt;
use crate::client::ModelClientSession;
use crate::client_common::ResponseEvent;
use crate::context_manager::estimate_response_item_model_visible_bytes;
use crate::context_manager::is_user_turn_boundary;
use crate::error::PraxisErr;
use crate::error::Result as PraxisResult;
use crate::event_mapping::is_contextual_user_message_content;
use crate::llm::prompts::LlmPromptPurpose;
use crate::llm::tasks::compact::CompactExecutionPolicy;
#[cfg(test)]
use crate::praxis::PreviousTurnSettings;
use crate::praxis::Session;
use crate::praxis::TurnContext;
use crate::turn_assistant_text::last_assistant_message_from_turn;
use crate::util::backoff;
use futures::prelude::*;
use praxis_protocol::items::ContextCompactionItem;
use praxis_protocol::items::TurnItem;
use praxis_protocol::models::BaseInstructions;
use praxis_protocol::models::ContentItem;
use praxis_protocol::models::FunctionCallOutputBody;
use praxis_protocol::models::ResponseItem;
use praxis_protocol::protocol::CompactedItem;
use praxis_protocol::protocol::EventMsg;
use praxis_protocol::protocol::TurnStartedEvent;
use praxis_protocol::protocol::WarningEvent;
use praxis_protocol::user_input::UserInput;
use praxis_utils_output_truncation::TruncationPolicy;
use praxis_utils_output_truncation::approx_token_count;
use praxis_utils_output_truncation::truncate_text;
use praxis_utils_output_truncation::truncate_utf8_bytes_with_omitted_marker;
use serde_json::Value;
use tracing::error;

pub const SUMMARIZATION_PROMPT: &str = include_str!("../templates/compact/prompt.md");
pub const UPDATE_SUMMARIZATION_PROMPT: &str = include_str!("../templates/compact/update.md");
const SUMMARIZATION_SYSTEM_PROMPT: &str = include_str!("../templates/compact/system.md");
pub const SUMMARY_PREFIX: &str = include_str!("../templates/compact/summary_prefix.md");
const COMPACT_USER_MESSAGE_MAX_TOKENS: usize = 20_000;
const LOCAL_COMPACT_KEEP_RECENT_TOKENS: i64 = 20_000;
const LOCAL_COMPACT_TOOL_RESULT_MAX_CHARS: usize = 2_000;

/// Controls whether compaction replacement history must include initial context.
///
/// Pre-turn/manual compaction variants use `DoNotInject`: they replace history with a summary and
/// clear `reference_context_item`, so the next regular turn will fully reinject initial context
/// after compaction.
///
/// Mid-turn compaction must use `BeforeLastUserMessage` because the model is trained to see the
/// compaction summary as the last item in history after mid-turn compaction; we therefore inject
/// initial context into the replacement history just above the last real user message.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum InitialContextInjection {
    BeforeLastUserMessage,
    DoNotInject,
}

pub(crate) fn compact_execution_policy_for_turn(
    sess: &Session,
    turn_context: &TurnContext,
) -> CompactExecutionPolicy {
    sess.llm_runtime_catalog()
        .compact_execution_policy_for_model(
            &turn_context.model_info,
            &turn_context.config.model_provider_id,
            &turn_context.provider,
            product_profile_for_turn(turn_context),
        )
        .unwrap_or(CompactExecutionPolicy::LocalPrompt)
}

pub(crate) fn should_use_remote_compact_task(sess: &Session, turn_context: &TurnContext) -> bool {
    compact_execution_policy_for_turn(sess, turn_context) == CompactExecutionPolicy::RemoteResponses
}

fn product_profile_for_turn(
    turn_context: &TurnContext,
) -> Option<crate::llm::ids::ProductProfileId> {
    turn_context
        .session_source
        .restriction_product()
        .and_then(crate::llm::ids::ProductProfileId::from_product)
}

fn compact_model_for_turn(sess: &Session, turn_context: &TurnContext) -> Option<String> {
    sess.llm_runtime_catalog().compact_model_for_model(
        &turn_context.model_info,
        &turn_context.config.model_provider_id,
        &turn_context.provider,
        product_profile_for_turn(turn_context),
    )
}

async fn local_compact_turn_context(
    sess: &Session,
    turn_context: &Arc<TurnContext>,
) -> Arc<TurnContext> {
    let Some(model) = compact_model_for_turn(sess, turn_context.as_ref())
        .as_deref()
        .map(str::trim)
        .filter(|model| !model.is_empty())
        .map(str::to_string)
    else {
        return turn_context.clone();
    };
    if turn_context.model_info.slug == model
        || turn_context.config.model.as_deref() == Some(model.as_str())
    {
        return turn_context.clone();
    }
    Arc::new(
        turn_context
            .with_model(model, &sess.services.models_manager)
            .await,
    )
}

pub(crate) async fn run_inline_auto_compact_task(
    sess: Arc<Session>,
    turn_context: Arc<TurnContext>,
    initial_context_injection: InitialContextInjection,
) -> PraxisResult<()> {
    run_compact_task_inner(
        sess,
        turn_context,
        LocalCompactMode::Auto,
        initial_context_injection,
    )
    .await?;
    Ok(())
}

pub(crate) async fn run_compact_task(
    sess: Arc<Session>,
    turn_context: Arc<TurnContext>,
    input: Vec<UserInput>,
) -> PraxisResult<()> {
    let start_event = EventMsg::TurnStarted(TurnStartedEvent {
        turn_id: turn_context.sub_id.clone(),
        model_context_window: turn_context.model_context_window(),
        collaboration_mode_kind: turn_context.collaboration_mode.mode,
    });
    sess.send_event(&turn_context, start_event).await;
    run_compact_task_inner(
        sess.clone(),
        turn_context,
        local_compact_mode_for_input(&input),
        InitialContextInjection::DoNotInject,
    )
    .await
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum LocalCompactMode {
    Auto,
    Manual,
}

async fn run_compact_task_inner(
    sess: Arc<Session>,
    turn_context: Arc<TurnContext>,
    mode: LocalCompactMode,
    initial_context_injection: InitialContextInjection,
) -> PraxisResult<()> {
    let compaction_item = TurnItem::ContextCompaction(ContextCompactionItem::new());
    sess.emit_turn_item_started(&turn_context, &compaction_item)
        .await;
    let compact_turn_context = local_compact_turn_context(sess.as_ref(), &turn_context).await;

    let history_snapshot = sess.clone_history().await;
    let raw_history_items = history_snapshot.raw_items().to_vec();
    let mut compact_plan = prepare_local_compaction(
        history_snapshot.for_prompt(&compact_turn_context.model_info.input_modalities),
        mode,
    );

    let mut truncated_count = 0usize;

    let max_retries = compact_turn_context.provider.stream_max_retries();
    let mut retries = 0;
    let compact_system_prompt = sess
        .llm_runtime_catalog()
        .resolve_prompt_for_model(
            &compact_turn_context.model_info,
            &compact_turn_context.config.model_provider_id,
            &compact_turn_context.provider,
            product_profile_for_turn(compact_turn_context.as_ref()),
            LlmPromptPurpose::Compact,
        )
        .unwrap_or_else(|| SUMMARIZATION_SYSTEM_PROMPT.to_string());
    let mut client_session = sess.services.model_runtime.new_session_for(
        &compact_turn_context.config.model_provider_id,
        &compact_turn_context.provider,
    );
    // Reuse one client session so turn-scoped state (sticky routing, websocket incremental
    // request tracking)
    // survives retries within this compact turn.

    loop {
        let summary_prompt = compact_plan.to_prompt_text(local_compact_instruction(
            compact_turn_context.as_ref(),
            compact_plan.previous_summary.is_some(),
        ));
        let prompt = Prompt {
            input: vec![ResponseItem::Message {
                id: None,
                role: "user".to_string(),
                content: vec![ContentItem::InputText {
                    text: summary_prompt,
                }],
                end_turn: None,
                phase: None,
            }],
            base_instructions: BaseInstructions {
                text: compact_system_prompt.clone(),
            },
            ..Default::default()
        };
        let turn_metadata_header = compact_turn_context
            .turn_metadata_state
            .current_header_value();
        let attempt_result = drain_to_completed(
            compact_turn_context.as_ref(),
            &mut client_session,
            turn_metadata_header.as_deref(),
            &prompt,
        )
        .await;

        match attempt_result {
            Ok(output_items) => {
                let summary = last_assistant_message_from_turn(&output_items)
                    .map(|text| text.trim().to_string())
                    .filter(|text| !text.is_empty());
                if summary.is_none() {
                    // A flaky provider can return a "successful" stream with no
                    // assistant message. Accepting that would replace the whole
                    // history with an empty summary and destroy the thread's
                    // context, so treat it like a stream failure instead.
                    let has_previous_summary = compact_plan
                        .previous_summary
                        .as_deref()
                        .is_some_and(|summary| !summary.trim().is_empty());
                    if retries < max_retries {
                        retries += 1;
                        let delay = backoff(retries);
                        sess.notify_stream_error(
                            turn_context.as_ref(),
                            format!("Reconnecting... {retries}/{max_retries}"),
                            PraxisErr::Stream(
                                "compaction model returned no summary".into(),
                                None,
                            ),
                        )
                        .await;
                        tokio::time::sleep(delay).await;
                        continue;
                    } else if !has_previous_summary {
                        let e = PraxisErr::Stream(
                            "compaction model returned no summary; keeping existing history"
                                .into(),
                            None,
                        );
                        let event = EventMsg::Error(e.to_error_event(/*message_prefix*/ None));
                        sess.send_event(&turn_context, event).await;
                        return Err(e);
                    }
                    // Retries exhausted but an earlier summary exists: fall
                    // through and let `take_summary_fallback` reuse it.
                    error!(
                        "compaction model returned no summary after {max_retries} retries; reusing previous summary"
                    );
                }
                compact_plan.summary_output = summary;
                if truncated_count > 0 {
                    sess.notify_background_event(
                        turn_context.as_ref(),
                        format!(
                            "Trimmed {truncated_count} older thread item(s) before compacting so the prompt fits the model context window."
                        ),
                    )
                    .await;
                }
                break;
            }
            Err(PraxisErr::Interrupted) => {
                return Err(PraxisErr::Interrupted);
            }
            Err(e @ PraxisErr::ContextWindowExceeded) => {
                if compact_plan.remove_oldest_summary_item() {
                    error!(
                        "Context window exceeded while compacting; removing oldest summary item. Error: {e}"
                    );
                    truncated_count += 1;
                    retries = 0;
                    continue;
                }
                sess.set_total_tokens_full(turn_context.as_ref()).await;
                let event = EventMsg::Error(e.to_error_event(/*message_prefix*/ None));
                sess.send_event(&turn_context, event).await;
                return Err(e);
            }
            Err(e) => {
                if retries < max_retries {
                    retries += 1;
                    let delay = backoff(retries);
                    sess.notify_stream_error(
                        turn_context.as_ref(),
                        format!("Reconnecting... {retries}/{max_retries}"),
                        e,
                    )
                    .await;
                    tokio::time::sleep(delay).await;
                    continue;
                } else {
                    let event = EventMsg::Error(e.to_error_event(/*message_prefix*/ None));
                    sess.send_event(&turn_context, event).await;
                    return Err(e);
                }
            }
        }
    }

    let summary_suffix = compact_plan.take_summary_fallback();
    let summary_text = format!("{SUMMARY_PREFIX}\n{}", summary_suffix.trim());

    let mut new_history =
        build_local_replacement_history(summary_text.clone(), compact_plan.retained_items);

    if matches!(
        initial_context_injection,
        InitialContextInjection::BeforeLastUserMessage
    ) {
        let initial_context = sess.build_initial_context(turn_context.as_ref()).await;
        new_history =
            insert_initial_context_before_last_real_user_or_summary(new_history, initial_context);
    }
    let ghost_snapshots: Vec<ResponseItem> = raw_history_items
        .iter()
        .filter(|item| matches!(item, ResponseItem::GhostSnapshot { .. }))
        .cloned()
        .collect();
    new_history.extend(ghost_snapshots);
    let reference_context_item = match initial_context_injection {
        InitialContextInjection::DoNotInject => None,
        InitialContextInjection::BeforeLastUserMessage => Some(turn_context.to_turn_context_item()),
    };
    let compacted_item = CompactedItem {
        message: summary_text.clone(),
        replacement_history: Some(new_history.clone()),
    };
    sess.replace_compacted_history(new_history, reference_context_item, compacted_item)
        .await;
    sess.recompute_token_usage(&turn_context).await;

    sess.emit_turn_item_completed(&turn_context, compaction_item)
        .await;
    let warning = EventMsg::Warning(WarningEvent {
        message: "Heads up: Long threads and multiple compactions can cause the model to be less accurate. Start a new thread when possible to keep threads small and targeted.".to_string(),
    });
    sess.send_event(&turn_context, warning).await;
    Ok(())
}

pub fn content_items_to_text(content: &[ContentItem]) -> Option<String> {
    let mut pieces = Vec::new();
    for item in content {
        match item {
            ContentItem::InputText { text } | ContentItem::OutputText { text } => {
                if !text.is_empty() {
                    pieces.push(text.as_str());
                }
            }
            ContentItem::InputImage { .. } => {}
        }
    }
    if pieces.is_empty() {
        None
    } else {
        Some(pieces.join("\n"))
    }
}

pub(crate) fn collect_user_messages(items: &[ResponseItem]) -> Vec<String> {
    items
        .iter()
        .filter_map(|item| match crate::event_mapping::parse_turn_item(item) {
            Some(TurnItem::UserMessage(user)) => {
                if is_summary_message(&user.message()) {
                    None
                } else {
                    Some(user.message())
                }
            }
            _ => None,
        })
        .collect()
}

pub(crate) fn is_summary_message(message: &str) -> bool {
    message.starts_with(format!("{SUMMARY_PREFIX}\n").as_str())
}

/// Inserts canonical initial context into compacted replacement history at the
/// model-expected boundary.
///
/// Placement rules:
/// - Prefer immediately before the last real user message.
/// - If no real user messages remain, insert before the compaction summary so
///   the summary stays last.
/// - If there are no user messages, insert before the last compaction item so
///   that item remains last (remote compaction may return only compaction items).
/// - If there are no user messages or compaction items, append the context.
pub(crate) fn insert_initial_context_before_last_real_user_or_summary(
    mut compacted_history: Vec<ResponseItem>,
    initial_context: Vec<ResponseItem>,
) -> Vec<ResponseItem> {
    let mut last_user_or_summary_index = None;
    let mut last_real_user_index = None;
    for (i, item) in compacted_history.iter().enumerate().rev() {
        let Some(TurnItem::UserMessage(user)) = crate::event_mapping::parse_turn_item(item) else {
            continue;
        };
        // Compaction summaries are encoded as user messages, so track both:
        // the last real user message (preferred insertion point) and the last
        // user-message-like item (fallback summary insertion point).
        last_user_or_summary_index.get_or_insert(i);
        if !is_summary_message(&user.message()) {
            last_real_user_index = Some(i);
            break;
        }
    }
    let last_compaction_index = compacted_history
        .iter()
        .enumerate()
        .rev()
        .find_map(|(i, item)| matches!(item, ResponseItem::Compaction { .. }).then_some(i));
    let insertion_index = last_real_user_index
        .or(last_user_or_summary_index)
        .or(last_compaction_index);

    // Re-inject canonical context from the current session since we stripped it
    // from the pre-compaction history. Prefer placing it before the last real
    // user message; if there is no real user message left, place it before the
    // summary or compaction item so the compaction item remains last.
    if let Some(insertion_index) = insertion_index {
        compacted_history.splice(insertion_index..insertion_index, initial_context);
    } else {
        compacted_history.extend(initial_context);
    }

    compacted_history
}

pub(crate) fn build_compacted_history(
    initial_context: Vec<ResponseItem>,
    user_messages: &[String],
    summary_text: &str,
) -> Vec<ResponseItem> {
    build_compacted_history_with_limit(
        initial_context,
        user_messages,
        summary_text,
        COMPACT_USER_MESSAGE_MAX_TOKENS,
    )
}

fn build_compacted_history_with_limit(
    mut history: Vec<ResponseItem>,
    user_messages: &[String],
    summary_text: &str,
    max_tokens: usize,
) -> Vec<ResponseItem> {
    let mut selected_messages: Vec<String> = Vec::new();
    if max_tokens > 0 {
        let mut remaining = max_tokens;
        for message in user_messages.iter().rev() {
            if remaining == 0 {
                break;
            }
            let tokens = approx_token_count(message);
            if tokens <= remaining {
                selected_messages.push(message.clone());
                remaining = remaining.saturating_sub(tokens);
            } else {
                let truncated = truncate_text(message, TruncationPolicy::Tokens(remaining));
                selected_messages.push(truncated);
                break;
            }
        }
        selected_messages.reverse();
    }

    for message in &selected_messages {
        history.push(ResponseItem::Message {
            id: None,
            role: "user".to_string(),
            content: vec![ContentItem::InputText {
                text: message.clone(),
            }],
            end_turn: None,
            phase: None,
        });
    }

    let summary_text = if summary_text.is_empty() {
        "(no summary available)".to_string()
    } else {
        summary_text.to_string()
    };

    history.push(ResponseItem::Message {
        id: None,
        role: "user".to_string(),
        content: vec![ContentItem::InputText { text: summary_text }],
        end_turn: None,
        phase: None,
    });

    history
}

fn local_compact_mode_for_input(_input: &[UserInput]) -> LocalCompactMode {
    LocalCompactMode::Manual
}

fn local_compact_instruction(turn_context: &TurnContext, has_previous_summary: bool) -> &str {
    turn_context
        .compact_prompt
        .as_deref()
        .map(str::trim)
        .filter(|prompt| !prompt.is_empty())
        .unwrap_or(if has_previous_summary {
            UPDATE_SUMMARIZATION_PROMPT
        } else {
            SUMMARIZATION_PROMPT
        })
}

#[derive(Debug)]
struct LocalCompactionPlan {
    summary_items: Vec<ResponseItem>,
    retained_items: Vec<ResponseItem>,
    previous_summary: Option<String>,
    summary_output: Option<String>,
}

impl LocalCompactionPlan {
    fn to_prompt_text(&self, final_instruction: &str) -> String {
        let conversation = serialize_compaction_conversation(
            &self.summary_items,
            LOCAL_COMPACT_TOOL_RESULT_MAX_CHARS,
        );
        let mut prompt = format!("<conversation>\n{conversation}\n</conversation>");
        if let Some(previous_summary) = &self.previous_summary {
            prompt.push_str("\n\n<previous-summary>\n");
            prompt.push_str(previous_summary.trim());
            prompt.push_str("\n</previous-summary>");
        }
        prompt.push_str("\n\n");
        prompt.push_str(final_instruction.trim());
        prompt
    }

    fn remove_oldest_summary_item(&mut self) -> bool {
        if self.summary_items.is_empty() {
            false
        } else {
            self.summary_items.remove(0);
            true
        }
    }

    fn take_summary_fallback(&mut self) -> String {
        self.summary_output
            .take()
            .or_else(|| self.previous_summary.clone())
            .filter(|summary| !summary.trim().is_empty())
            .unwrap_or_else(|| "(no summary available)".to_string())
    }
}

fn prepare_local_compaction(
    prompt_history: Vec<ResponseItem>,
    mode: LocalCompactMode,
) -> LocalCompactionPlan {
    let mut compactable_items = Vec::new();
    let mut previous_summary = None;
    for item in prompt_history {
        if let Some(summary) = summary_from_item(&item) {
            previous_summary = Some(summary);
            continue;
        }
        if should_include_in_local_compaction(&item) {
            compactable_items.push(item);
        }
    }

    let cut_index = choose_local_compaction_cut_index(&compactable_items, mode);
    let retained_items = compactable_items[cut_index..].to_vec();
    let summary_items = compactable_items[..cut_index].to_vec();

    LocalCompactionPlan {
        summary_items,
        retained_items,
        previous_summary,
        summary_output: None,
    }
}

fn choose_local_compaction_cut_index(items: &[ResponseItem], mode: LocalCompactMode) -> usize {
    if items.is_empty() {
        return 0;
    }
    if mode == LocalCompactMode::Manual {
        return items.len();
    }

    let mut accumulated_tokens = 0i64;
    let mut tentative = None;
    for (index, item) in items.iter().enumerate().rev() {
        accumulated_tokens =
            accumulated_tokens.saturating_add(estimate_item_tokens_for_local_compaction(item));
        if accumulated_tokens >= LOCAL_COMPACT_KEEP_RECENT_TOKENS {
            tentative = Some(index);
            break;
        }
    }

    let Some(tentative) = tentative else {
        return items.len();
    };

    if let Some(user_boundary) = (tentative..items.len())
        .find(|index| is_user_turn_boundary(&items[*index]) && !is_summary_item(&items[*index]))
    {
        return user_boundary;
    }

    if let Some(valid_start) =
        (tentative..items.len()).find(|index| is_valid_retained_suffix_start(&items[*index]))
    {
        return valid_start;
    }

    if is_tool_output(&items[tentative])
        && let Some(call_index) = matching_tool_call_index_before(items, tentative)
    {
        return call_index;
    }

    0
}

fn estimate_item_tokens_for_local_compaction(item: &ResponseItem) -> i64 {
    let bytes = estimate_response_item_model_visible_bytes(item);
    bytes.saturating_add(3) / 4
}

fn should_include_in_local_compaction(item: &ResponseItem) -> bool {
    match item {
        ResponseItem::Message { role, content, .. } if role == "developer" => false,
        ResponseItem::Message { role, content, .. } if role == "system" => false,
        ResponseItem::Message { role, content, .. } if role == "user" => {
            !is_contextual_user_message_content(content) && !is_summary_item(item)
        }
        ResponseItem::GhostSnapshot { .. } | ResponseItem::Other => false,
        ResponseItem::Compaction { .. } => false,
        ResponseItem::Message { .. }
        | ResponseItem::Reasoning { .. }
        | ResponseItem::LocalShellCall { .. }
        | ResponseItem::FunctionCall { .. }
        | ResponseItem::ToolSearchCall { .. }
        | ResponseItem::ToolSearchOutput { .. }
        | ResponseItem::CustomToolCall { .. }
        | ResponseItem::CustomToolCallOutput { .. }
        | ResponseItem::FunctionCallOutput { .. }
        | ResponseItem::WebSearchCall { .. }
        | ResponseItem::ImageGenerationCall { .. } => true,
    }
}

fn is_valid_retained_suffix_start(item: &ResponseItem) -> bool {
    match item {
        ResponseItem::Message { role, content, .. } if role == "user" => {
            !is_contextual_user_message_content(content) && !is_summary_item(item)
        }
        ResponseItem::Message { role, .. } if role == "assistant" => true,
        ResponseItem::FunctionCall { .. }
        | ResponseItem::CustomToolCall { .. }
        | ResponseItem::ToolSearchCall { .. }
        | ResponseItem::LocalShellCall { .. }
        | ResponseItem::WebSearchCall { .. }
        | ResponseItem::ImageGenerationCall { .. } => true,
        _ => false,
    }
}

fn matching_tool_call_index_before(items: &[ResponseItem], output_index: usize) -> Option<usize> {
    let output_call_id = output_call_id(&items[output_index])?;
    items[..output_index]
        .iter()
        .enumerate()
        .rev()
        .find_map(|(index, item)| (tool_call_id(item) == Some(output_call_id)).then_some(index))
}

fn output_call_id(item: &ResponseItem) -> Option<&str> {
    match item {
        ResponseItem::FunctionCallOutput { call_id, .. }
        | ResponseItem::CustomToolCallOutput { call_id, .. } => Some(call_id),
        ResponseItem::ToolSearchOutput {
            call_id: Some(call_id),
            ..
        } => Some(call_id),
        _ => None,
    }
}

fn tool_call_id(item: &ResponseItem) -> Option<&str> {
    match item {
        ResponseItem::FunctionCall { call_id, .. }
        | ResponseItem::CustomToolCall { call_id, .. } => Some(call_id),
        ResponseItem::ToolSearchCall {
            call_id: Some(call_id),
            ..
        } => Some(call_id),
        ResponseItem::LocalShellCall {
            call_id: Some(call_id),
            ..
        } => Some(call_id),
        _ => None,
    }
}

fn is_tool_output(item: &ResponseItem) -> bool {
    matches!(
        item,
        ResponseItem::FunctionCallOutput { .. }
            | ResponseItem::CustomToolCallOutput { .. }
            | ResponseItem::ToolSearchOutput { .. }
    )
}

fn summary_from_item(item: &ResponseItem) -> Option<String> {
    let ResponseItem::Message { role, content, .. } = item else {
        return None;
    };
    if role != "user" {
        return None;
    }
    let text = content_items_to_text(content)?;
    strip_summary_prefix(&text)
        .map(str::trim)
        .map(str::to_string)
}

fn is_summary_item(item: &ResponseItem) -> bool {
    summary_from_item(item).is_some()
}

fn strip_summary_prefix(message: &str) -> Option<&str> {
    message.strip_prefix(format!("{SUMMARY_PREFIX}\n").as_str())
}

fn build_local_replacement_history(
    summary_text: String,
    retained_items: Vec<ResponseItem>,
) -> Vec<ResponseItem> {
    let mut history = Vec::with_capacity(retained_items.len().saturating_add(1));
    history.push(ResponseItem::Message {
        id: None,
        role: "user".to_string(),
        content: vec![ContentItem::InputText { text: summary_text }],
        end_turn: None,
        phase: None,
    });
    history.extend(retained_items);
    history
}

fn serialize_compaction_conversation(
    items: &[ResponseItem],
    tool_result_max_chars: usize,
) -> String {
    let mut parts = Vec::new();
    for item in items {
        match item {
            ResponseItem::Message { role, content, .. } if role == "user" => {
                if let Some(text) = content_items_to_text(content) {
                    parts.push(format!("[User]: {text}"));
                }
            }
            ResponseItem::Message { role, content, .. } if role == "assistant" => {
                if let Some(text) = content_items_to_text(content) {
                    parts.push(format!("[Assistant]: {text}"));
                }
            }
            ResponseItem::Reasoning {
                summary, content, ..
            } => {
                let raw_content = content
                    .as_ref()
                    .map(|content| {
                        content
                            .iter()
                            .map(|entry| match entry {
                                praxis_protocol::models::ReasoningItemContent::ReasoningText {
                                    text,
                                }
                                | praxis_protocol::models::ReasoningItemContent::Text { text } => {
                                    text.as_str()
                                }
                            })
                            .collect::<Vec<_>>()
                            .join("\n")
                    })
                    .filter(|text| !text.trim().is_empty());
                if let Some(text) = raw_content {
                    parts.push(format!("[Assistant thinking]: {text}"));
                } else if !summary.is_empty() {
                    let summary = summary
                        .iter()
                        .map(|entry| {
                            match entry {
                            praxis_protocol::models::ReasoningItemReasoningSummary::SummaryText {
                                text,
                            } => text.as_str(),
                        }
                        })
                        .collect::<Vec<_>>()
                        .join("\n");
                    parts.push(format!("[Assistant thinking summary]: {summary}"));
                }
            }
            ResponseItem::FunctionCall {
                name, arguments, ..
            } => {
                parts.push(format!("[Assistant tool call]: {name}({arguments})"));
            }
            ResponseItem::CustomToolCall { name, input, .. } => {
                parts.push(format!("[Assistant tool call]: {name}({input})"));
            }
            ResponseItem::ToolSearchCall {
                execution,
                arguments,
                ..
            } => {
                parts.push(format!(
                    "[Assistant tool search]: {execution}({})",
                    json_to_compact_string(arguments)
                ));
            }
            ResponseItem::LocalShellCall { action, .. } => {
                parts.push(format!(
                    "[Assistant local shell]: {}",
                    serde_json::to_string(action)
                        .unwrap_or_else(|_| "<unserializable>".to_string())
                ));
            }
            ResponseItem::FunctionCallOutput { output, .. }
            | ResponseItem::CustomToolCallOutput { output, .. } => {
                if let Some(text) = function_output_to_text(output) {
                    parts.push(format!(
                        "[Tool result]: {}",
                        truncate_utf8_bytes_with_omitted_marker(&text, tool_result_max_chars)
                    ));
                }
            }
            ResponseItem::ToolSearchOutput { tools, .. } => {
                parts.push(format!(
                    "[Tool search result]: {}",
                    truncate_utf8_bytes_with_omitted_marker(
                        &json_to_compact_string(&Value::Array(tools.clone())),
                        tool_result_max_chars
                    )
                ));
            }
            ResponseItem::WebSearchCall { action, .. } => {
                parts.push(format!(
                    "[Web search]: {}",
                    serde_json::to_string(action)
                        .unwrap_or_else(|_| "<unserializable>".to_string())
                ));
            }
            ResponseItem::ImageGenerationCall {
                revised_prompt,
                result,
                ..
            } => {
                parts.push(format!(
                    "[Image generation]: prompt={}; result={}",
                    revised_prompt.as_deref().unwrap_or(""),
                    result
                ));
            }
            ResponseItem::Message { .. }
            | ResponseItem::Compaction { .. }
            | ResponseItem::GhostSnapshot { .. }
            | ResponseItem::Other => {}
        }
    }

    if parts.is_empty() {
        "(no prior conversation content)".to_string()
    } else {
        parts.join("\n\n")
    }
}

fn function_output_to_text(
    output: &praxis_protocol::models::FunctionCallOutputPayload,
) -> Option<String> {
    match &output.body {
        FunctionCallOutputBody::Text(text) => Some(text.clone()),
        FunctionCallOutputBody::ContentItems(items) => {
            let text = items
                .iter()
                .filter_map(|item| match item {
                    praxis_protocol::models::FunctionCallOutputContentItem::InputText { text } => {
                        Some(text.as_str())
                    }
                    praxis_protocol::models::FunctionCallOutputContentItem::InputImage {
                        ..
                    } => None,
                })
                .collect::<Vec<_>>()
                .join("\n");
            (!text.is_empty()).then_some(text)
        }
    }
}

fn json_to_compact_string(value: &Value) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "<unserializable>".to_string())
}

async fn drain_to_completed(
    turn_context: &TurnContext,
    client_session: &mut ModelClientSession,
    turn_metadata_header: Option<&str>,
    prompt: &Prompt,
) -> PraxisResult<Vec<ResponseItem>> {
    let mut stream = client_session
        .stream(
            prompt,
            &turn_context.model_info,
            &turn_context.session_telemetry,
            turn_context.reasoning_effort,
            turn_context.reasoning_summary,
            turn_context.config.service_tier,
            turn_metadata_header,
        )
        .await?;
    let mut output_items = Vec::new();
    loop {
        let maybe_event = stream.next().await;
        let Some(event) = maybe_event else {
            return Err(PraxisErr::Stream(
                "stream closed before response.completed".into(),
                None,
            ));
        };
        match event {
            Ok(ResponseEvent::OutputItemDone(item)) => {
                output_items.push(item);
            }
            Ok(ResponseEvent::ServerReasoningIncluded(included)) => {
                let _ = included;
            }
            Ok(ResponseEvent::RateLimits(snapshot)) => {
                let _ = snapshot;
            }
            Ok(ResponseEvent::Completed { token_usage, .. }) => {
                let _ = token_usage;
                return Ok(output_items);
            }
            Ok(_) => continue,
            Err(e) => return Err(e),
        }
    }
}

#[cfg(test)]
#[path = "compact_tests.rs"]
mod tests;
