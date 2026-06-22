use std::time::Duration;

use crate::exec::ExecToolCallOutput;
use crate::exec::StreamOutput;

pub(super) fn cancelled_output(message: String) -> ExecToolCallOutput {
    terminal_output(message)
}

pub(super) fn failed_output(message: String) -> ExecToolCallOutput {
    terminal_output(message)
}

fn terminal_output(message: String) -> ExecToolCallOutput {
    ExecToolCallOutput {
        exit_code: -1,
        stdout: StreamOutput::new(String::new()),
        stderr: StreamOutput::new(message.clone()),
        aggregated_output: StreamOutput::new(message),
        model_output: None,
        duration: Duration::ZERO,
        timed_out: false,
        agent_os_artifact_id: None,
        raw_output_spool: None,
    }
}
