use crate::protocol::api::CollabAgentState;
use crate::protocol::api::CollabAgentTool;
use crate::protocol::api::CollabAgentToolCallStatus;
use crate::protocol::api::CommandAction;
use crate::protocol::api::CommandExecutionStatus;
use crate::protocol::api::DynamicToolCallOutputContentItem;
use crate::protocol::api::DynamicToolCallStatus;
use crate::protocol::api::FileUpdateChange;
use crate::protocol::api::McpToolCallError;
use crate::protocol::api::McpToolCallResult;
use crate::protocol::api::McpToolCallStatus;
use crate::protocol::api::PatchApplyStatus;
use crate::protocol::api::PatchChangeKind;
use crate::protocol::api::ThreadItem;
use crate::protocol::api::Turn;
use crate::protocol::api::TurnError as ApiTurnError;
use crate::protocol::api::TurnError;
use crate::protocol::api::TurnStatus;
use crate::protocol::api::UserInput;
use crate::protocol::api::WebSearchAction;
use praxis_protocol::items::parse_hook_prompt_message;
use praxis_protocol::models::MessagePhase;
use praxis_protocol::protocol::AgentReasoningEvent;
use praxis_protocol::protocol::AgentReasoningRawContentEvent;
use praxis_protocol::protocol::AgentStatus;
use praxis_protocol::protocol::ApplyPatchApprovalRequestEvent;
use praxis_protocol::protocol::CollabAgentInteractionKind;
use praxis_protocol::protocol::CompactedItem;
use praxis_protocol::protocol::ContextCompactedEvent;
use praxis_protocol::protocol::DynamicToolCallResponseEvent;
use praxis_protocol::protocol::ErrorEvent;
use praxis_protocol::protocol::EventMsg;
use praxis_protocol::protocol::ExecCommandBeginEvent;
use praxis_protocol::protocol::ExecCommandEndEvent;
use praxis_protocol::protocol::ImageGenerationBeginEvent;
use praxis_protocol::protocol::ImageGenerationEndEvent;
use praxis_protocol::protocol::ItemCompletedEvent;
use praxis_protocol::protocol::ItemStartedEvent;
use praxis_protocol::protocol::McpToolCallBeginEvent;
use praxis_protocol::protocol::McpToolCallEndEvent;
use praxis_protocol::protocol::PatchApplyBeginEvent;
use praxis_protocol::protocol::PatchApplyEndEvent;
use praxis_protocol::protocol::ReviewOutputEvent;
use praxis_protocol::protocol::RolloutItem;
use praxis_protocol::protocol::ThreadRolledBackEvent;
use praxis_protocol::protocol::TurnAbortedEvent;
use praxis_protocol::protocol::TurnCompleteEvent;
use praxis_protocol::protocol::TurnStartedEvent;
use praxis_protocol::protocol::UserMessageEvent;
use praxis_protocol::protocol::ViewImageToolCallEvent;
use praxis_protocol::protocol::WebSearchBeginEvent;
use praxis_protocol::protocol::WebSearchEndEvent;
use std::collections::HashMap;
use std::collections::VecDeque;
use tracing::warn;
use uuid::Uuid;

mod builder_state;
mod collaboration_events;
mod conversions;
mod item_events;
mod pending_turn;
mod turn_events;

use conversions::REVIEW_FALLBACK_MESSAGE;
use conversions::convert_dynamic_tool_content_items;
pub use conversions::convert_patch_changes;
use conversions::render_review_output_text;
use pending_turn::PendingTurn;
use pending_turn::upsert_turn_item;

/// Convert persisted [`RolloutItem`] entries into a sequence of [`Turn`] values.
///
/// When available, this uses `TurnContext.turn_id` as the canonical turn id so
/// resumed/rebuilt thread history preserves the original turn identifiers.
pub fn build_turns_from_rollout_items(items: &[RolloutItem]) -> Vec<Turn> {
    let mut builder = ThreadHistoryBuilder::new();
    for item in items {
        builder.handle_rollout_item(item);
    }
    builder.finish()
}

pub fn build_recent_turns_from_rollout_items(
    items: &[RolloutItem],
    turn_limit: usize,
) -> Vec<Turn> {
    if turn_limit == 0 {
        return Vec::new();
    }
    let mut builder = ThreadHistoryBuilder::with_max_finished_turns(turn_limit);
    for item in items {
        builder.handle_rollout_item(item);
    }
    builder.finish()
}

fn collab_interaction_tool(kind: CollabAgentInteractionKind) -> CollabAgentTool {
    match kind {
        CollabAgentInteractionKind::SendMessage => CollabAgentTool::SendMessage,
        CollabAgentInteractionKind::AssignTask => CollabAgentTool::AssignTask,
    }
}

pub struct ThreadHistoryBuilder {
    turns: Vec<Turn>,
    current_turn: Option<PendingTurn>,
    next_item_index: i64,
    current_rollout_index: usize,
    next_rollout_index: usize,
    max_finished_turns: Option<usize>,
    dropped_turns: VecDeque<Turn>,
}

impl Default for ThreadHistoryBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl ThreadHistoryBuilder {
    pub fn new() -> Self {
        Self {
            turns: Vec::new(),
            current_turn: None,
            next_item_index: 1,
            current_rollout_index: 0,
            next_rollout_index: 0,
            max_finished_turns: None,
            dropped_turns: VecDeque::new(),
        }
    }

    pub fn with_max_finished_turns(max_finished_turns: usize) -> Self {
        let mut builder = Self::new();
        builder.max_finished_turns = Some(max_finished_turns);
        builder
    }

    pub fn reset(&mut self) {
        let max_finished_turns = self.max_finished_turns;
        *self = Self::new();
        self.max_finished_turns = max_finished_turns;
    }

    pub fn finish(mut self) -> Vec<Turn> {
        self.finish_current_turn();
        self.trim_finished_turns();
        self.turns
    }

    pub fn active_turn_snapshot(&self) -> Option<Turn> {
        self.current_turn
            .as_ref()
            .map(Turn::from)
            .or_else(|| self.turns.last().cloned())
    }

    pub fn has_active_turn(&self) -> bool {
        self.current_turn.is_some()
    }

    pub fn active_turn_id_if_explicit(&self) -> Option<String> {
        self.current_turn
            .as_ref()
            .filter(|turn| turn.opened_explicitly)
            .map(|turn| turn.id.clone())
    }

    pub fn active_turn_start_index(&self) -> Option<usize> {
        self.current_turn
            .as_ref()
            .map(|turn| turn.rollout_start_index)
    }

    /// Shared reducer for persisted rollout replay and in-memory current-turn
    /// tracking used by running thread resume/rejoin.
    ///
    /// This function should handle all EventMsg variants that can be persisted in a rollout file.
    /// See `should_persist_event_msg` in `praxis-rs/core/rollout/policy.rs`.
    pub fn handle_event(&mut self, event: &EventMsg) {
        match event {
            EventMsg::UserMessage(payload) => self.handle_user_message(payload),
            EventMsg::AgentMessage(payload) => self.handle_agent_message(
                payload.message.clone(),
                payload.phase.clone(),
                payload.memory_citation.clone().map(Into::into),
            ),
            EventMsg::AgentReasoning(payload) => self.handle_agent_reasoning(payload),
            EventMsg::AgentReasoningRawContent(payload) => {
                self.handle_agent_reasoning_raw_content(payload)
            }
            EventMsg::WebSearchBegin(payload) => self.handle_web_search_begin(payload),
            EventMsg::WebSearchEnd(payload) => self.handle_web_search_end(payload),
            EventMsg::ExecCommandBegin(payload) => self.handle_exec_command_begin(payload),
            EventMsg::ExecCommandEnd(payload) => self.handle_exec_command_end(payload),
            EventMsg::ApplyPatchApprovalRequest(payload) => {
                self.handle_apply_patch_approval_request(payload)
            }
            EventMsg::PatchApplyBegin(payload) => self.handle_patch_apply_begin(payload),
            EventMsg::PatchApplyEnd(payload) => self.handle_patch_apply_end(payload),
            EventMsg::DynamicToolCallRequest(payload) => {
                self.handle_dynamic_tool_call_request(payload)
            }
            EventMsg::DynamicToolCallResponse(payload) => {
                self.handle_dynamic_tool_call_response(payload)
            }
            EventMsg::McpToolCallBegin(payload) => self.handle_mcp_tool_call_begin(payload),
            EventMsg::McpToolCallEnd(payload) => self.handle_mcp_tool_call_end(payload),
            EventMsg::ViewImageToolCall(payload) => self.handle_view_image_tool_call(payload),
            EventMsg::ImageGenerationBegin(payload) => self.handle_image_generation_begin(payload),
            EventMsg::ImageGenerationEnd(payload) => self.handle_image_generation_end(payload),
            EventMsg::CollabAgentSpawnBegin(payload) => {
                self.handle_collab_agent_spawn_begin(payload)
            }
            EventMsg::CollabAgentSpawnEnd(payload) => self.handle_collab_agent_spawn_end(payload),
            EventMsg::CollabAgentInteractionBegin(payload) => {
                self.handle_collab_agent_interaction_begin(payload)
            }
            EventMsg::CollabAgentInteractionEnd(payload) => {
                self.handle_collab_agent_interaction_end(payload)
            }
            EventMsg::CollabWaitingBegin(payload) => self.handle_collab_waiting_begin(payload),
            EventMsg::CollabWaitingEnd(payload) => self.handle_collab_waiting_end(payload),
            EventMsg::CollabCloseBegin(payload) => self.handle_collab_close_begin(payload),
            EventMsg::CollabCloseEnd(payload) => self.handle_collab_close_end(payload),
            EventMsg::CollabResumeBegin(payload) => self.handle_collab_resume_begin(payload),
            EventMsg::CollabResumeEnd(payload) => self.handle_collab_resume_end(payload),
            EventMsg::ContextCompacted(payload) => self.handle_context_compacted(payload),
            EventMsg::EnteredReviewMode(payload) => self.handle_entered_review_mode(payload),
            EventMsg::ExitedReviewMode(payload) => self.handle_exited_review_mode(payload),
            EventMsg::ItemStarted(payload) => self.handle_item_started(payload),
            EventMsg::ItemCompleted(payload) => self.handle_item_completed(payload),
            EventMsg::HookStarted(_) | EventMsg::HookCompleted(_) => {}
            EventMsg::Error(payload) => self.handle_error(payload),
            EventMsg::TokenCount(_) => {}
            EventMsg::ThreadRolledBack(payload) => self.handle_thread_rollback(payload),
            EventMsg::UndoCompleted(_) => {}
            EventMsg::TurnAborted(payload) => self.handle_turn_aborted(payload),
            EventMsg::TurnStarted(payload) => self.handle_turn_started(payload),
            EventMsg::TurnComplete(payload) => self.handle_turn_complete(payload),
            _ => {}
        }
    }

    pub fn handle_rollout_item(&mut self, item: &RolloutItem) {
        self.current_rollout_index = self.next_rollout_index;
        self.next_rollout_index += 1;
        match item {
            RolloutItem::EventMsg(event) => self.handle_event(event),
            RolloutItem::Compacted(payload) => self.handle_compacted(payload),
            RolloutItem::ResponseItem(item) => self.handle_response_item(item),
            RolloutItem::TurnContext(_) | RolloutItem::SessionMeta(_) => {}
        }
        self.trim_finished_turns();
    }
}

#[cfg(test)]
#[path = "thread_history_tests.rs"]
mod tests;
