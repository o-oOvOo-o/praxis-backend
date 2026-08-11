use crate::tools::context::ToolPayload;
use praxis_shell_command::delay_probe_fingerprint;
use std::sync::Mutex;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;

const MAX_EQUIVALENT_DELAY_PROBES: usize = 2;
const DELAY_PROBE_BLOCK_MESSAGE: &str = "Repeated delay-and-status polling was blocked after two equivalent attempts in this turn. Do not start another sleep/poll process. Resume an existing Praxis background session with write_stdin and a long yield, or use one bounded event-driven wait command for the external process. Keep the wait interruptible and continue from that single wait instead of busy-polling.";

#[derive(Debug)]
struct ShellWaitProbeStreak {
    fingerprint: String,
    count: usize,
}

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
    shell_wait_probe_streak: Mutex<Option<ShellWaitProbeStreak>>,
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

    pub(crate) fn record_shell_wait_probe(
        &self,
        tool_name: &str,
        payload: &ToolPayload,
    ) -> ToolLoopDecision {
        if !matches!(tool_name, "shell_command" | "exec_command") {
            return ToolLoopDecision::Allow;
        }

        let fingerprint = shell_command_text(tool_name, payload)
            .as_deref()
            .and_then(delay_probe_fingerprint);
        let mut streak = self.shell_wait_probe_streak_guard();
        let Some(fingerprint) = fingerprint else {
            *streak = None;
            return ToolLoopDecision::Allow;
        };

        let count = match streak.as_mut() {
            Some(streak) if streak.fingerprint == fingerprint => {
                streak.count += 1;
                streak.count
            }
            _ => {
                *streak = Some(ShellWaitProbeStreak {
                    fingerprint,
                    count: 1,
                });
                1
            }
        };

        if count <= MAX_EQUIVALENT_DELAY_PROBES {
            ToolLoopDecision::Allow
        } else {
            ToolLoopDecision::Block {
                message: DELAY_PROBE_BLOCK_MESSAGE.to_string(),
            }
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

    fn shell_wait_probe_streak_guard(
        &self,
    ) -> std::sync::MutexGuard<'_, Option<ShellWaitProbeStreak>> {
        self.shell_wait_probe_streak
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

fn shell_command_text(tool_name: &str, payload: &ToolPayload) -> Option<String> {
    let ToolPayload::Function { arguments } = payload else {
        return None;
    };
    let arguments: serde_json::Value = serde_json::from_str(arguments).ok()?;
    arguments
        .get(if tool_name == "exec_command" {
            "cmd"
        } else {
            "command"
        })?
        .as_str()
        .map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shell_command(command: &str) -> ToolPayload {
        ToolPayload::Function {
            arguments: serde_json::json!({ "command": command }).to_string(),
        }
    }

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

    #[test]
    fn equivalent_delay_probes_are_allowed_twice_then_blocked() {
        let guard = ToolLoopGuardState::default();

        assert_eq!(
            guard.record_shell_wait_probe(
                "shell_command",
                &shell_command("Start-Sleep -Seconds 45; Get-Process cargo,rustc")
            ),
            ToolLoopDecision::Allow
        );
        assert_eq!(
            guard.record_shell_wait_probe(
                "shell_command",
                &shell_command("  start-sleep   -Seconds 55 ;  get-process cargo,rustc  ")
            ),
            ToolLoopDecision::Allow
        );
        let ToolLoopDecision::Block { message } = guard.record_shell_wait_probe(
            "shell_command",
            &shell_command("Start-Sleep -Seconds 50; Get-Process cargo,rustc"),
        ) else {
            panic!("third equivalent delay probe should be blocked");
        };

        assert!(message.contains("Repeated delay-and-status polling"));
        assert!(message.contains("write_stdin"));
        assert!(message.contains("event-driven wait"));
    }

    #[test]
    fn productive_tool_call_resets_delay_probe_streak() {
        let guard = ToolLoopGuardState::default();
        let delayed_probe = shell_command("Start-Sleep -Seconds 55; Get-Process cargo,rustc");

        assert_eq!(
            guard.record_shell_wait_probe("shell_command", &delayed_probe),
            ToolLoopDecision::Allow
        );
        assert_eq!(
            guard.record_shell_wait_probe("shell_command", &delayed_probe),
            ToolLoopDecision::Allow
        );
        assert_eq!(
            guard.record_shell_wait_probe(
                "shell_command",
                &shell_command("Get-Content build.log -Tail 20")
            ),
            ToolLoopDecision::Allow
        );
        assert_eq!(
            guard.record_shell_wait_probe("shell_command", &delayed_probe),
            ToolLoopDecision::Allow
        );
    }

    #[test]
    fn text_mentions_and_event_driven_waits_are_not_delay_probes() {
        let guard = ToolLoopGuardState::default();

        for command in [
            "rg 'Start-Sleep' core/src",
            "Wait-Process -Id 42 -Timeout 300",
            "Start-Sleep -Seconds 1; Write-Output ready",
            "sleep 1; echo ready",
        ] {
            for _ in 0..3 {
                assert_eq!(
                    guard.record_shell_wait_probe("shell_command", &shell_command(command)),
                    ToolLoopDecision::Allow
                );
            }
        }
    }
}
