use std::sync::atomic::AtomicBool;
use std::sync::atomic::AtomicU64;

pub(super) struct SessionAutomationState {
    pub(super) next_internal_sub_id: AtomicU64,
    pub(super) auto_title_attempted: AtomicBool,
    pub(super) auto_summary_in_flight: AtomicBool,
}

pub(super) fn build() -> SessionAutomationState {
    SessionAutomationState {
        next_internal_sub_id: AtomicU64::new(0),
        auto_title_attempted: AtomicBool::new(false),
        auto_summary_in_flight: AtomicBool::new(false),
    }
}
