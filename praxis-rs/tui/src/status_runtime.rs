//! Helpers for deriving factual status-row content and animation policy.

use std::time::Duration;

pub(crate) const GENERIC_STATUS_HEADER: &str = "Turn running";
pub(crate) const STATUS_ANIMATION_FRAME_DELAY_FOCUSED: Duration = Duration::from_millis(80);
pub(crate) const STATUS_ANIMATION_FRAME_DELAY_UNFOCUSED: Duration = Duration::from_millis(160);
