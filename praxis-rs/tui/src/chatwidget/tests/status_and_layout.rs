use super::*;
use pretty_assertions::assert_eq;
use ratatui::buffer::Buffer;
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;

#[path = "status_and_layout/active_status_and_errors.rs"]
mod active_status_and_errors;
#[path = "status_and_layout/context_and_basic_status.rs"]
mod context_and_basic_status;
#[path = "status_and_layout/hooks_and_layout_snapshots.rs"]
mod hooks_and_layout_snapshots;
#[path = "status_and_layout/rate_limits_and_cache.rs"]
mod rate_limits_and_cache;
#[path = "status_and_layout/status_line_and_title.rs"]
mod status_line_and_title;
#[path = "status_and_layout/transcript_rendering.rs"]
mod transcript_rendering;
