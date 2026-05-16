//! Shared runtime state for the TUI's in-flight turn surfaces.
//!
//! The chat widget, status row, team-task bridge, and toast/notification paths
//! all surface pieces of the same "what is the agent doing right now?" model.
//! This module keeps that shared state small and integration-friendly so
//! high-touch orchestration code can pass around one snapshot instead of
//! re-deriving strings in several places.

use std::collections::VecDeque;

const DEFAULT_ACTIVITY_TRAIL_LIMIT: usize = 3;

fn compact_footer_note(message: &str) -> Option<String> {
    let trimmed = message.trim();
    if trimmed.is_empty() {
        return None;
    }

    let stripped = trimmed
        .strip_prefix("Tip:")
        .or_else(|| trimmed.strip_prefix("Tip"))
        .map(str::trim_start)
        .unwrap_or(trimmed);

    (!stripped.is_empty()).then(|| stripped.to_string())
}

fn format_next_task_message(subject: &str) -> Option<String> {
    let subject = subject.trim();
    (!subject.is_empty()).then(|| format!("Up next: {subject}"))
}

/// Mirrors the status-detail capitalization policy used by the status widget
/// without depending on that render module directly.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum RuntimeTextCapitalization {
    #[default]
    CapitalizeFirst,
    Preserve,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RuntimeTaskSnapshot {
    pub(crate) id: String,
    pub(crate) subject: String,
    pub(crate) active_form: Option<String>,
}

impl RuntimeTaskSnapshot {
    pub(crate) fn new(
        id: impl Into<String>,
        subject: impl Into<String>,
        active_form: Option<String>,
    ) -> Self {
        Self {
            id: id.into(),
            subject: subject.into(),
            active_form: active_form
                .map(|active_form| active_form.trim().to_string())
                .filter(|active_form| !active_form.is_empty()),
        }
    }

    pub(crate) fn display_message(&self) -> Option<String> {
        self.active_form
            .clone()
            .or_else(|| (!self.subject.trim().is_empty()).then(|| self.subject.clone()))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct StatusIndicatorSnapshot {
    pub(crate) header: String,
    pub(crate) details: Option<String>,
    pub(crate) details_capitalization: RuntimeTextCapitalization,
    pub(crate) details_max_lines: usize,
    pub(crate) activity_message: Option<String>,
    pub(crate) extra_lines: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ActivityTrail {
    entries: VecDeque<String>,
    limit: usize,
}

impl Default for ActivityTrail {
    fn default() -> Self {
        Self::new(DEFAULT_ACTIVITY_TRAIL_LIMIT)
    }
}

impl ActivityTrail {
    pub(crate) fn new(limit: usize) -> Self {
        Self {
            entries: VecDeque::new(),
            limit: limit.max(1),
        }
    }

    pub(crate) fn clear(&mut self) {
        self.entries.clear();
    }

    pub(crate) fn push(&mut self, next: String) {
        let next = next.trim().to_string();
        if next.is_empty() {
            return;
        }
        if self
            .entries
            .back()
            .is_some_and(|existing| existing == &next)
        {
            return;
        }
        self.entries.push_back(next);
        while self.entries.len() > self.limit {
            self.entries.pop_front();
        }
    }

    pub(crate) fn summary(&self) -> Option<String> {
        (!self.entries.is_empty()).then(|| {
            self.entries
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>()
                .join(" → ")
        })
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TurnRuntimeState {
    base_header: String,
    details: Option<String>,
    details_capitalization: RuntimeTextCapitalization,
    details_max_lines: usize,
    tip_message: Option<String>,
    budget_message: Option<String>,
    summary_message: Option<String>,
    queue_preview_message: Option<String>,
    activity_trail: ActivityTrail,
    active_task: Option<RuntimeTaskSnapshot>,
    next_task: Option<RuntimeTaskSnapshot>,
}

impl Default for TurnRuntimeState {
    fn default() -> Self {
        Self::new()
    }
}

impl TurnRuntimeState {
    const WORKING_HEADER: &str = "Working";

    pub(crate) fn new() -> Self {
        Self {
            base_header: Self::WORKING_HEADER.to_string(),
            details: None,
            details_capitalization: RuntimeTextCapitalization::CapitalizeFirst,
            details_max_lines: 3,
            tip_message: None,
            budget_message: None,
            summary_message: None,
            queue_preview_message: None,
            activity_trail: ActivityTrail::new(DEFAULT_ACTIVITY_TRAIL_LIMIT),
            active_task: None,
            next_task: None,
        }
    }

    pub(crate) fn set_base_status(
        &mut self,
        header: String,
        details: Option<String>,
        details_capitalization: RuntimeTextCapitalization,
        details_max_lines: usize,
    ) {
        self.base_header = if header.trim().is_empty() {
            Self::WORKING_HEADER.to_string()
        } else {
            header
        };
        self.details = details
            .map(|details| details.trim().to_string())
            .filter(|details| !details.is_empty());
        self.details_capitalization = details_capitalization;
        self.details_max_lines = details_max_lines.max(1);
    }

    pub(crate) fn set_tip_message(&mut self, message: Option<String>) {
        self.tip_message = message
            .map(|message| message.trim().to_string())
            .filter(|message| !message.is_empty());
    }

    pub(crate) fn set_budget_message(&mut self, message: Option<String>) {
        self.budget_message = message
            .map(|message| message.trim().to_string())
            .filter(|message| !message.is_empty());
    }

    pub(crate) fn set_summary_message(&mut self, message: Option<String>) {
        self.summary_message = message
            .map(|message| message.trim().to_string())
            .filter(|message| !message.is_empty());
    }

    pub(crate) fn set_queue_preview_message(&mut self, message: Option<String>) {
        self.queue_preview_message = message
            .map(|message| message.trim().to_string())
            .filter(|message| !message.is_empty());
    }

    pub(crate) fn set_active_task(&mut self, task: Option<RuntimeTaskSnapshot>) {
        self.active_task = task;
    }

    pub(crate) fn set_next_task(&mut self, task: Option<RuntimeTaskSnapshot>) {
        self.next_task = task;
    }

    pub(crate) fn clear_tasks(&mut self) {
        self.active_task = None;
        self.next_task = None;
    }

    pub(crate) fn clear_activity(&mut self) {
        self.activity_trail.clear();
    }

    pub(crate) fn push_activity(&mut self, summary: String) {
        self.activity_trail.push(summary);
    }

    fn resolved_header(&self) -> String {
        if self.base_header != Self::WORKING_HEADER {
            return self.base_header.clone();
        }

        self.active_task
            .as_ref()
            .and_then(RuntimeTaskSnapshot::display_message)
            .unwrap_or_else(|| Self::WORKING_HEADER.to_string())
    }

    fn extra_lines(&self) -> Vec<String> {
        let mut lines = Vec::new();
        if let Some(budget_message) = &self.budget_message {
            lines.push(budget_message.clone());
        }

        if let Some(summary_message) = &self.summary_message {
            lines.push(summary_message.clone());
        }

        if let Some(next_task) = &self.next_task {
            if !self
                .active_task
                .as_ref()
                .is_some_and(|active| active.id == next_task.id)
            {
                if let Some(message) = format_next_task_message(&next_task.subject) {
                    lines.push(message);
                }
            }
        } else if let Some(tip_message) = &self.tip_message {
            if let Some(message) = compact_footer_note(tip_message) {
                lines.push(message);
            }
        }

        if let Some(queue_preview_message) = &self.queue_preview_message {
            lines.push(queue_preview_message.clone());
        }

        lines
    }

    pub(crate) fn status_snapshot(&self) -> StatusIndicatorSnapshot {
        StatusIndicatorSnapshot {
            header: self.resolved_header(),
            details: self.details.clone(),
            details_capitalization: self.details_capitalization,
            details_max_lines: self.details_max_lines,
            activity_message: self.activity_trail.summary(),
            extra_lines: self.extra_lines(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use pretty_assertions::assert_eq;

    #[test]
    fn active_task_overrides_generic_working_header() {
        let mut runtime = TurnRuntimeState::new();
        runtime.set_active_task(Some(RuntimeTaskSnapshot::new(
            "task-1",
            "Run tests",
            Some("Running tests".to_string()),
        )));

        assert_eq!(
            runtime.status_snapshot().header,
            "Running tests".to_string()
        );
    }

    #[test]
    fn explicit_header_beats_active_task() {
        let mut runtime = TurnRuntimeState::new();
        runtime.set_base_status(
            "Reviewing approval request".to_string(),
            None,
            RuntimeTextCapitalization::CapitalizeFirst,
            3,
        );
        runtime.set_active_task(Some(RuntimeTaskSnapshot::new(
            "task-1",
            "Run tests",
            Some("Running tests".to_string()),
        )));

        assert_eq!(
            runtime.status_snapshot().header,
            "Reviewing approval request".to_string()
        );
    }

    #[test]
    fn activity_trail_dedupes_adjacent_entries() {
        let mut trail = ActivityTrail::new(3);
        trail.push("Checking".to_string());
        trail.push("Checking".to_string());
        trail.push("Editing".to_string());

        assert_eq!(trail.summary(), Some("Checking → Editing".to_string()));
    }

    #[test]
    fn next_task_becomes_tip_fallback_when_missing() {
        let mut runtime = TurnRuntimeState::new();
        runtime.set_tip_message(Some("Use /clear to reset stale context".to_string()));

        assert_eq!(
            runtime.status_snapshot().extra_lines,
            vec!["Use /clear to reset stale context".to_string()]
        );
    }

    #[test]
    fn next_task_footer_uses_compact_copy() {
        let mut runtime = TurnRuntimeState::new();
        runtime.set_next_task(Some(RuntimeTaskSnapshot::new("task-2", "Audit diff", None)));

        assert_eq!(
            runtime.status_snapshot().extra_lines,
            vec!["Up next: Audit diff".to_string()]
        );
    }

    #[test]
    fn tip_prefix_is_stripped_from_footer_copy() {
        let mut runtime = TurnRuntimeState::new();
        runtime.set_tip_message(Some("Tip: Use /status if this looks stuck".to_string()));

        assert_eq!(
            runtime.status_snapshot().extra_lines,
            vec!["Use /status if this looks stuck".to_string()]
        );
    }

    #[test]
    fn summary_message_stacks_above_next_task() {
        let mut runtime = TurnRuntimeState::new();
        runtime.set_summary_message(Some("Test Team · 1 teammate".to_string()));
        runtime.set_next_task(Some(RuntimeTaskSnapshot::new("task-2", "Audit diff", None)));

        assert_eq!(
            runtime.status_snapshot().extra_lines,
            vec![
                "Test Team · 1 teammate".to_string(),
                "Up next: Audit diff".to_string(),
            ]
        );
    }

    #[test]
    fn queue_preview_message_stacks_after_next_task() {
        let mut runtime = TurnRuntimeState::new();
        runtime.set_next_task(Some(RuntimeTaskSnapshot::new("task-2", "Audit diff", None)));
        runtime.set_queue_preview_message(Some(
            "Queue: Write regression test, Review logs".to_string(),
        ));

        assert_eq!(
            runtime.status_snapshot().extra_lines,
            vec![
                "Up next: Audit diff".to_string(),
                "Queue: Write regression test, Review logs".to_string(),
            ]
        );
    }
}
