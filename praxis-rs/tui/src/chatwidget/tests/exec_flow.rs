use super::*;
use pretty_assertions::assert_eq;

#[path = "exec_flow/apply_patch_flow.rs"]
mod apply_patch_flow;
#[path = "exec_flow/approval_decisions.rs"]
mod approval_decisions;
#[path = "exec_flow/interrupts_and_patch_events.rs"]
mod interrupts_and_patch_events;
#[path = "exec_flow/shell_and_modal.rs"]
mod shell_and_modal;
#[path = "exec_flow/status_and_history.rs"]
mod status_and_history;
#[path = "exec_flow/unified_exec_waits.rs"]
mod unified_exec_waits;
