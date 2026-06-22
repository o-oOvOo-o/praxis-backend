mod diff;
mod token_ledger;

pub use diff::TurnDiffTracker;
pub use token_ledger::TokenLedger;

use serde::Deserialize;
use serde::Serialize;

use crate::guard::LoopGuard;
use crate::guard::ToolCallAdmission;
use crate::model::TokenUsage;
use crate::model::TurnItem;
use crate::outcome::TurnCompletionMessage;

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct TurnState {
    transcript_delta: Vec<TurnItem>,
    token_usage: TokenLedger,
    tool_call_count: u64,
    round_count: u64,
    last_agent_message: Option<String>,
    guard: LoopGuard,
    diff: TurnDiffTracker,
}

impl TurnState {
    pub fn with_guard(mut self, guard: LoopGuard) -> Self {
        self.guard = guard;
        self
    }

    pub fn transcript_delta(&self) -> &[TurnItem] {
        self.transcript_delta.as_slice()
    }

    pub fn token_usage(&self) -> &TokenLedger {
        &self.token_usage
    }

    pub fn round_count(&self) -> u64 {
        self.round_count
    }

    pub fn tool_call_count(&self) -> u64 {
        self.tool_call_count
    }

    pub fn diff(&self) -> &TurnDiffTracker {
        &self.diff
    }

    pub fn start_round(&mut self) -> u64 {
        self.round_count = self.round_count.saturating_add(1);
        self.round_count
    }

    pub fn record_tool_calls(&mut self, count: usize) -> ToolCallAdmission {
        self.tool_call_count = self
            .tool_call_count
            .saturating_add(u64::try_from(count).unwrap_or(u64::MAX));
        self.guard.admit_tool_calls(self.tool_call_count)
    }

    pub fn record_items(&mut self, items: impl IntoIterator<Item = TurnItem>) {
        let mut items = items.into_iter().peekable();
        if items.peek().is_none() {
            return;
        }
        self.transcript_delta.extend(items);
        self.diff.mark_changed();
    }

    pub fn record_usage(&mut self, usage: &TokenUsage) {
        self.token_usage.record_usage(usage);
    }

    pub fn mark_transcript_delta_absorbed_by_prompt_refresh(&mut self) {
        self.transcript_delta.clear();
    }

    pub fn record_last_agent_message(&mut self, message: impl Into<String>) {
        self.last_agent_message = Some(message.into());
    }

    pub fn record_completion_message(&mut self, message: TurnCompletionMessage) {
        if let Some(message) = message.into_option() {
            self.record_last_agent_message(message);
        }
    }

    pub fn last_agent_message(&self) -> Option<&str> {
        self.last_agent_message.as_deref()
    }

    pub fn last_completion_message(&self) -> TurnCompletionMessage {
        TurnCompletionMessage::from_option(self.last_agent_message.clone())
    }

    pub fn into_last_agent_message(self) -> Option<String> {
        self.last_agent_message
    }
}
