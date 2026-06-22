//! Shared runtime state for the TUI's in-flight turn surfaces.
//!
//! The chat widget, status row, and toast/notification paths
//! all surface pieces of the same "what is the agent doing right now?" model.
//! This module keeps that shared state small and integration-friendly so
//! high-touch orchestration code can pass around one snapshot instead of
//! re-deriving strings in several places.

use std::collections::VecDeque;

use crate::status_runtime::GENERIC_STATUS_HEADER;

const DEFAULT_ACTIVITY_TRAIL_LIMIT: usize = 3;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct StatusIndicatorSnapshot {
    pub(crate) header: String,
    pub(crate) details: Option<String>,
    pub(crate) details_max_lines: usize,
    pub(crate) activity_message: Option<String>,
    pub(crate) footer_message: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ActivityTrail {
    entries: VecDeque<String>,
    limit: usize,
}

impl ActivityTrail {
    fn new(limit: usize) -> Self {
        Self {
            entries: VecDeque::new(),
            limit: limit.max(1),
        }
    }

    fn clear(&mut self) {
        self.entries.clear();
    }

    fn push(&mut self, next: String) -> bool {
        let next = next.trim().to_string();
        if next.is_empty() {
            return false;
        }
        if self
            .entries
            .back()
            .is_some_and(|existing| existing == &next)
        {
            return false;
        }
        self.entries.push_back(next);
        while self.entries.len() > self.limit {
            self.entries.pop_front();
        }
        true
    }

    fn summary(&self) -> Option<String> {
        let mut entries = self.entries.iter();
        let first = entries.next()?;
        let mut summary = first.clone();
        for entry in entries {
            summary.push_str(" → ");
            summary.push_str(entry);
        }
        Some(summary)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TurnRuntimeState {
    base_header: String,
    details: Option<String>,
    details_max_lines: usize,
    budget_message: Option<String>,
    activity_trail: ActivityTrail,
}

impl Default for TurnRuntimeState {
    fn default() -> Self {
        Self::new()
    }
}

impl TurnRuntimeState {
    pub(crate) fn new() -> Self {
        Self {
            base_header: GENERIC_STATUS_HEADER.to_string(),
            details: None,
            details_max_lines: 3,
            budget_message: None,
            activity_trail: ActivityTrail::new(DEFAULT_ACTIVITY_TRAIL_LIMIT),
        }
    }

    pub(crate) fn set_base_status(
        &mut self,
        header: String,
        details: Option<String>,
        details_max_lines: usize,
    ) {
        self.base_header = if header.trim().is_empty() {
            GENERIC_STATUS_HEADER.to_string()
        } else {
            header
        };
        self.details = details
            .map(|details| details.trim().to_string())
            .filter(|details| !details.is_empty());
        self.details_max_lines = details_max_lines.max(1);
    }

    pub(crate) fn set_budget_message(&mut self, message: Option<String>) {
        self.budget_message = message
            .map(|message| message.trim().to_string())
            .filter(|message| !message.is_empty());
    }

    pub(crate) fn clear_activity(&mut self) {
        self.activity_trail.clear();
    }

    pub(crate) fn push_activity(&mut self, summary: String) -> bool {
        self.activity_trail.push(summary)
    }

    pub(crate) fn activity_summary(&self) -> Option<String> {
        self.activity_trail.summary()
    }

    pub(crate) fn status_snapshot(&self) -> StatusIndicatorSnapshot {
        StatusIndicatorSnapshot {
            header: self.base_header.clone(),
            details: self.details.clone(),
            details_max_lines: self.details_max_lines,
            activity_message: self.activity_trail.summary(),
            footer_message: self.budget_message.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use pretty_assertions::assert_eq;

    #[test]
    fn empty_base_header_falls_back_to_generic_status() {
        let mut runtime = TurnRuntimeState::new();
        runtime.set_base_status("   ".to_string(), None, 3);

        assert_eq!(runtime.status_snapshot().header, GENERIC_STATUS_HEADER);
    }

    #[test]
    fn explicit_header_is_preserved() {
        let mut runtime = TurnRuntimeState::new();
        runtime.set_base_status("Reviewing approval request".to_string(), None, 3);

        assert_eq!(
            runtime.status_snapshot().header,
            "Reviewing approval request".to_string()
        );
    }

    #[test]
    fn activity_trail_dedupes_adjacent_entries() {
        let mut trail = ActivityTrail::new(3);
        assert!(trail.push("tool shell started".to_string()));
        assert!(!trail.push("tool shell started".to_string()));
        assert!(trail.push("tool shell completed".to_string()));

        assert_eq!(
            trail.summary(),
            Some("tool shell started → tool shell completed".to_string())
        );
    }

    #[test]
    fn budget_message_becomes_footer_message() {
        let mut runtime = TurnRuntimeState::new();
        runtime.set_budget_message(Some("87% context used".to_string()));

        assert_eq!(
            runtime.status_snapshot().footer_message,
            Some("87% context used".to_string())
        );
    }
}
