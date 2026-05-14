//! Helpers for deriving richer status-row content and animation policy.

use std::time::Duration;

use crate::turn_runtime::TurnStatusSnapshot;

pub(crate) const GENERIC_STATUS_HEADER: &str = "Working";
pub(crate) const STATUS_ANIMATION_FRAME_DELAY_FOCUSED: Duration = Duration::from_millis(80);
pub(crate) const STATUS_ANIMATION_FRAME_DELAY_UNFOCUSED: Duration = Duration::from_millis(160);

const FOCUSED_GENERIC_VERBS: [&str; 6] = [
    "Working",
    "Thinking",
    "Analyzing",
    "Planning",
    "Editing",
    "Checking",
];
const UNFOCUSED_GENERIC_VERBS: [&str; 3] = ["Working", "Thinking", "Still working"];
const STATUS_SLOW_THRESHOLD_SECS: u64 = 30;
const STATUS_STALLED_THRESHOLD_SECS: u64 = 90;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum StatusAlert {
    #[default]
    None,
    Slow,
    Stalled,
}

pub(crate) fn status_alert(elapsed: Duration) -> StatusAlert {
    match elapsed.as_secs() {
        secs if secs >= STATUS_STALLED_THRESHOLD_SECS => StatusAlert::Stalled,
        secs if secs >= STATUS_SLOW_THRESHOLD_SECS => StatusAlert::Slow,
        _ => StatusAlert::None,
    }
}

pub(crate) fn generic_status_verb(elapsed: Duration, terminal_focused: bool) -> &'static str {
    let verbs = if terminal_focused {
        &FOCUSED_GENERIC_VERBS[..]
    } else {
        &UNFOCUSED_GENERIC_VERBS[..]
    };
    let frame = usize::try_from(elapsed.as_secs() / 4).unwrap_or(0);
    verbs[frame % verbs.len()]
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct StatusRenderModel {
    pub(crate) header: String,
    pub(crate) details: Option<String>,
    pub(crate) activity_message: Option<String>,
    pub(crate) footer_message: Option<String>,
}

impl StatusRenderModel {
    pub(crate) fn from_snapshot(
        fallback_header: &str,
        details: Option<String>,
        activity_message: Option<String>,
        snapshot: &TurnStatusSnapshot,
    ) -> Self {
        Self {
            header: snapshot.effective_header(fallback_header),
            details: snapshot.effective_details(details),
            activity_message,
            footer_message: snapshot.effective_footer(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::turn_runtime::CurrentTaskSnapshot;
    use crate::turn_runtime::NextTaskSnapshot;

    use pretty_assertions::assert_eq;

    #[test]
    fn footer_message_prefers_next_task() {
        let snapshot = TurnStatusSnapshot {
            next_task: Some(NextTaskSnapshot {
                team_id: "team".to_string(),
                task_id: "task".to_string(),
                title: "Audit diff".to_string(),
            }),
            footer_note: Some("Tip: /clear".to_string()),
            ..TurnStatusSnapshot::default()
        };

        let model = StatusRenderModel::from_snapshot(
            GENERIC_STATUS_HEADER,
            /*details*/ None,
            /*activity_message*/ None,
            &snapshot,
        );

        assert_eq!(
            model.footer_message,
            Some("Up next: Audit diff".to_string())
        );
    }

    #[test]
    fn current_task_description_fills_empty_details() {
        let snapshot = TurnStatusSnapshot {
            current_task: Some(CurrentTaskSnapshot {
                team_id: "team".to_string(),
                task_id: "task".to_string(),
                title: "Audit diff".to_string(),
                description: Some("Inspect the rendered patch".to_string()),
                active_form: Some("Inspecting rendered patch".to_string()),
            }),
            ..TurnStatusSnapshot::default()
        };

        let model = StatusRenderModel::from_snapshot(
            GENERIC_STATUS_HEADER,
            /*details*/ None,
            /*activity_message*/ None,
            &snapshot,
        );

        assert_eq!(
            model.details,
            Some("Inspect the rendered patch".to_string())
        );
        assert_eq!(model.header, "Inspecting rendered patch".to_string());
    }

    #[test]
    fn footer_message_strips_tip_prefix() {
        let snapshot = TurnStatusSnapshot {
            footer_note: Some("Tip: Run /status for the live breakdown".to_string()),
            ..TurnStatusSnapshot::default()
        };

        let model = StatusRenderModel::from_snapshot(
            GENERIC_STATUS_HEADER,
            /*details*/ None,
            /*activity_message*/ None,
            &snapshot,
        );

        assert_eq!(
            model.footer_message,
            Some("Run /status for the live breakdown".to_string())
        );
    }
}
