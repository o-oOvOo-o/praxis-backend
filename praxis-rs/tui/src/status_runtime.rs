//! Helpers for deriving richer status-row content and animation policy.

use std::time::Duration;

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

#[cfg(test)]
mod tests {
    use super::*;

    use pretty_assertions::assert_eq;

    #[test]
    fn alert_thresholds_track_elapsed_time() {
        assert_eq!(status_alert(Duration::from_secs(0)), StatusAlert::None);
        assert_eq!(status_alert(Duration::from_secs(30)), StatusAlert::Slow);
        assert_eq!(status_alert(Duration::from_secs(90)), StatusAlert::Stalled);
    }
}
