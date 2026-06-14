use std::fmt;
use std::panic::AssertUnwindSafe;
use std::panic::catch_unwind;
use std::path::Path;

use praxis_shell_command::parse_command::shlex_join;

use crate::agent_os::AgentOsExecutionOpenRequest;
use crate::agent_os::ManagedCommandSpan;
use crate::exec::ExecToolCallOutput;
use crate::tools::output_reducer::apply_command_output_reduction;
use crate::tools::sandboxing::ToolCtx;
use crate::tools::sandboxing::ToolError;

pub(crate) async fn start_agent_os_command_span_with_runtime_route(
    ctx: &ToolCtx,
    command: &[String],
    cwd: &Path,
    process_id: Option<i32>,
    runtime_kind: &'static str,
    runtime_owner_id: Option<&str>,
) -> Result<ManagedCommandSpan, ToolError> {
    ctx.session
        .services
        .agent_os
        .open_execution(AgentOsExecutionOpenRequest {
            thread_id: ctx.session.conversation_id,
            command: shlex_join(command),
            argv: command,
            cwd,
            process_id,
            runtime_kind: Some(runtime_kind),
            runtime_owner_id,
        })
        .await
        .map_err(|err| ToolError::Rejected(err.to_string()))
}

/// Finish a managed span for an exec output and attach the created AgentOS
/// artifact id back to the output. All command runtimes should use this helper
/// so output artifact handling stays identical across shell/unified backends.
pub(crate) async fn finish_agent_os_span_with_output(
    span: &ManagedCommandSpan,
    output: &mut ExecToolCallOutput,
    runtime_label: &str,
) {
    let fallback_output = output.aggregated_output.text.as_bytes();
    let finish_result = if let Some(spool) = output.raw_output_spool.take() {
        span.finish_with_spooled_output(Some(output.exit_code), spool, fallback_output)
            .await
    } else {
        span.finish(Some(output.exit_code), fallback_output).await
    };
    match finish_result {
        Ok(artifact_id) => output.agent_os_artifact_id = artifact_id,
        Err(err) => {
            tracing::warn!("failed to finish AgentOS {runtime_label} command span: {err}");
        }
    }
    if let Some(raw_command) = span.raw_command().await {
        if catch_unwind(AssertUnwindSafe(|| {
            apply_command_output_reduction(raw_command.as_str(), output);
        }))
        .is_err()
        {
            tracing::warn!(
                "AgentOS {runtime_label} command output reducer panicked; preserving raw output"
            );
        }
    }
}

/// Finish a managed span after backend failure. This keeps failed command
/// accounting, lease release, and artifact creation identical across runtimes.
pub(crate) async fn finish_failed_agent_os_span(
    span: &ManagedCommandSpan,
    runtime_label: &str,
    err: &impl fmt::Debug,
) {
    let error_text = format!("{err:?}");
    if let Err(finish_err) = span.finish_failure(error_text.as_bytes()).await {
        tracing::warn!(
            "failed to finish failed AgentOS {runtime_label} command span: {finish_err}"
        );
    }
}

/// Finish a managed span that was intentionally abandoned before the backend
/// spawned a process. Used by fallback paths so preflight/ticket state does not
/// leak when the preferred backend declines to run.
pub(crate) async fn finish_abandoned_agent_os_span(
    span: &ManagedCommandSpan,
    runtime_label: &str,
    reason: &'static [u8],
) {
    if let Err(err) = span.finish_failure(reason).await {
        tracing::warn!("failed to finish abandoned AgentOS {runtime_label} command span: {err}");
    }
}
