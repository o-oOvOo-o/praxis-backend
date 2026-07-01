use std::sync::Mutex;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;

#[derive(Debug, Default)]
pub(crate) struct ToolLoopGuardState {
    empty_model_completions: AtomicUsize,
    subagent_tool_calls_seen: AtomicUsize,
    any_tool_call_seen: std::sync::atomic::AtomicBool,
    terminal_list_agents_calls: AtomicUsize,
    suppress_list_agents: std::sync::atomic::AtomicBool,
    suppress_all_tools: std::sync::atomic::AtomicBool,
    terminal_model_error: Mutex<Option<String>>,
    pending_followup_intervention: Mutex<Option<String>>,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum ToolLoopDecision {
    Allow,
    Block { message: String },
}

impl ToolLoopGuardState {
    pub(crate) fn record_empty_model_completion(&self) -> Option<String> {
        let count = self.empty_model_completions.fetch_add(1, Ordering::Relaxed) + 1;
        (count == 1).then(|| {
            "The previous model response ended with no assistant text and no tool calls. Re-read the latest user message and act now. If the request requires tools, call the required tools explicitly; otherwise provide a concrete final answer. Do not end the turn empty.".to_string()
        })
    }

    pub(crate) fn record_tool_call(&self, tool_name: &str) {
        self.any_tool_call_seen.store(true, Ordering::Relaxed);
        if matches!(
            tool_name,
            "spawn_agent"
                | "wait_agent"
                | "assign_task"
                | "send_message"
                | "close_agent"
                | "list_agents"
        ) {
            self.subagent_tool_calls_seen
                .fetch_add(1, Ordering::Relaxed);
        }
    }

    pub(crate) fn has_any_tool_call(&self) -> bool {
        self.any_tool_call_seen.load(Ordering::Relaxed)
    }

    pub(crate) fn record_list_agents_terminal(
        &self,
        should_stop_listing: bool,
    ) -> ToolLoopDecision {
        if !should_stop_listing {
            self.terminal_list_agents_calls.store(0, Ordering::Relaxed);
            self.suppress_list_agents.store(false, Ordering::Relaxed);
            self.suppress_all_tools.store(false, Ordering::Relaxed);
            *self.pending_followup_intervention_guard() = None;
            return ToolLoopDecision::Allow;
        }

        self.suppress_list_agents.store(true, Ordering::Relaxed);
        let count = self
            .terminal_list_agents_calls
            .fetch_add(1, Ordering::Relaxed)
            + 1;

        let message = if count == 1 {
            "list_agents returned a terminal empty state in this turn. No live sub-agents or \
             pending AgentOS work remain. Stop calling tools now and provide the final answer, \
             including any completion marker requested by the user."
                .to_string()
        } else {
            format!(
                "list_agents already returned a terminal empty state {count} times in this turn. \
                 No live sub-agents or pending AgentOS work remain. Stop calling tools now and provide \
                 the final answer, including any completion marker requested by the user."
            )
        };
        *self.pending_followup_intervention_guard() = Some(message.clone());

        if count == 1 {
            return ToolLoopDecision::Allow;
        }

        self.suppress_all_tools.store(true, Ordering::Relaxed);
        ToolLoopDecision::Block { message }
    }

    pub(crate) fn should_hide_tool(&self, tool_name: &str) -> bool {
        self.suppress_all_tools.load(Ordering::Relaxed)
            || (tool_name == "list_agents" && self.suppress_list_agents.load(Ordering::Relaxed))
    }

    pub(crate) fn has_terminal_list_agents(&self) -> bool {
        self.terminal_list_agents_calls.load(Ordering::Relaxed) > 0
    }

    pub(crate) fn has_subagent_tool_calls(&self) -> bool {
        self.subagent_tool_calls_seen.load(Ordering::Relaxed) > 0
    }

    pub(crate) fn record_terminal_model_error(&self, message: String) {
        *self.terminal_model_error_guard() = Some(message);
    }

    pub(crate) fn has_terminal_model_error(&self) -> bool {
        self.terminal_model_error_guard().is_some()
    }

    pub(crate) fn terminal_model_error_message(&self) -> Option<String> {
        self.terminal_model_error_guard().clone()
    }

    pub(crate) fn take_followup_intervention(&self) -> Option<String> {
        self.pending_followup_intervention_guard().take()
    }

    fn terminal_model_error_guard(&self) -> std::sync::MutexGuard<'_, Option<String>> {
        self.terminal_model_error
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn pending_followup_intervention_guard(&self) -> std::sync::MutexGuard<'_, Option<String>> {
        self.pending_followup_intervention
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_list_agents_suppresses_visibility_then_blocks_repeats() {
        let guard = ToolLoopGuardState::default();

        assert_eq!(
            guard.record_list_agents_terminal(true),
            ToolLoopDecision::Allow
        );
        assert!(guard.should_hide_tool("list_agents"));
        assert_eq!(
            guard.take_followup_intervention(),
            Some(
                "list_agents returned a terminal empty state in this turn. No live sub-agents or pending AgentOS work remain. Stop calling tools now and provide the final answer, including any completion marker requested by the user.".to_string()
            )
        );
        let ToolLoopDecision::Block { message } = guard.record_list_agents_terminal(true) else {
            panic!("second terminal list_agents call should be blocked");
        };

        assert!(message.contains("terminal empty state 2 times"));
        assert!(guard.should_hide_tool("spawn_agent"));
        assert_eq!(guard.take_followup_intervention(), Some(message));
        assert_eq!(guard.take_followup_intervention(), None);
    }

    #[test]
    fn non_terminal_list_agents_resets_terminal_counter() {
        let guard = ToolLoopGuardState::default();

        assert_eq!(
            guard.record_list_agents_terminal(true),
            ToolLoopDecision::Allow
        );
        assert_eq!(
            guard.record_list_agents_terminal(false),
            ToolLoopDecision::Allow
        );
        assert!(!guard.should_hide_tool("list_agents"));
        assert!(!guard.should_hide_tool("spawn_agent"));
        assert_eq!(
            guard.record_list_agents_terminal(true),
            ToolLoopDecision::Allow
        );
    }

    #[test]
    fn empty_model_completion_intervenes_once() {
        let guard = ToolLoopGuardState::default();

        let first = guard.record_empty_model_completion();
        assert!(
            first
                .as_deref()
                .unwrap_or_default()
                .contains("ended with no assistant text")
        );
        assert_eq!(guard.record_empty_model_completion(), None);
    }

    #[test]
    fn terminal_model_error_is_recorded() {
        let guard = ToolLoopGuardState::default();

        assert!(!guard.has_terminal_model_error());
        guard.record_terminal_model_error("exceeded retry limit".to_string());

        assert!(guard.has_terminal_model_error());
        assert_eq!(
            guard.terminal_model_error_message().as_deref(),
            Some("exceeded retry limit")
        );
    }
}
