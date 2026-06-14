//! Helpers for deriving factual status-row content and animation policy.

use std::time::Duration;

pub(crate) const GENERIC_STATUS_HEADER: &str = "Turn running";
pub(crate) const STATUS_ANIMATION_FRAME_DELAY_FOCUSED: Duration = Duration::from_millis(80);
pub(crate) const STATUS_ANIMATION_FRAME_DELAY_UNFOCUSED: Duration = Duration::from_millis(160);

const STATUS_SLOW_THRESHOLD_SECS: u64 = 30;
const STATUS_LONG_RUNNING_THRESHOLD_SECS: u64 = 90;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum StatusAlert {
    #[default]
    None,
    Slow,
    LongRunning,
}

pub(crate) fn status_alert(elapsed: Duration) -> StatusAlert {
    match elapsed.as_secs() {
        secs if secs >= STATUS_LONG_RUNNING_THRESHOLD_SECS => StatusAlert::LongRunning,
        secs if secs >= STATUS_SLOW_THRESHOLD_SECS => StatusAlert::Slow,
        _ => StatusAlert::None,
    }
}

pub(crate) fn generic_status_verb(_elapsed: Duration, _terminal_focused: bool) -> &'static str {
    GENERIC_STATUS_HEADER
}

#[cfg(test)]
mod tests {
    use super::*;

    use pretty_assertions::assert_eq;

    #[test]
    fn alert_thresholds_track_elapsed_time() {
        assert_eq!(status_alert(Duration::from_secs(0)), StatusAlert::None);
        assert_eq!(status_alert(Duration::from_secs(30)), StatusAlert::Slow);
        assert_eq!(status_alert(Duration::from_secs(90)), StatusAlert::LongRunning);
    }
}
