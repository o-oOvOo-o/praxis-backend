use std::sync::Arc;

use praxis_async_utils::CancelErr;
use praxis_async_utils::OrCancelExt;
use praxis_protocol::protocol::ExecCommandStatus;
use tokio_util::sync::CancellationToken;
use tracing::error;

use super::events;
use super::execution_plan::UserShellExecutionPlan;
use super::output::cancelled_output;
use super::output::failed_output;
use super::persistence::persist_user_shell_output;
use super::types::UserShellCommandMode;
use crate::exec::execute_exec_request;
use crate::praxis::Session;
use crate::praxis::TurnContext;
use crate::tools::format_exec_output_str;

pub(super) async fn execute_user_shell_command(
    session: Arc<Session>,
    turn_context: Arc<TurnContext>,
    command: String,
    cancellation_token: CancellationToken,
    mode: UserShellCommandMode,
) {
    session
        .services
        .session_telemetry
        .counter("praxis.task.user_shell", /*inc*/ 1, &[]);

    events::send_turn_started_if_needed(&session, turn_context.as_ref(), mode).await;

    let plan = UserShellExecutionPlan::build(&session, turn_context.as_ref(), command);
    events::send_exec_begin(&session, turn_context.as_ref(), &plan).await;

    let exec_result = execute_exec_request(
        plan.exec_request(&session, turn_context.as_ref()),
        plan.stdout_stream(&session, turn_context.as_ref()),
        /*after_spawn*/ None,
    )
    .or_cancel(&cancellation_token)
    .await;

    match exec_result {
        Err(CancelErr::Cancelled) => {
            let aborted_message = "command aborted by user".to_string();
            let exec_output = cancelled_output(aborted_message.clone());
            persist_user_shell_output(
                &session,
                turn_context.as_ref(),
                &plan.raw_command,
                &exec_output,
                mode,
            )
            .await;
            events::send_exec_end(
                &session,
                turn_context.as_ref(),
                &plan,
                &exec_output,
                aborted_message,
                ExecCommandStatus::Failed,
            )
            .await;
        }
        Ok(Ok(output)) => {
            let status = if output.exit_code == 0 {
                ExecCommandStatus::Completed
            } else {
                ExecCommandStatus::Failed
            };
            let formatted_output = format_exec_output_str(&output, turn_context.truncation_policy);
            events::send_exec_end(
                &session,
                turn_context.as_ref(),
                &plan,
                &output,
                formatted_output,
                status,
            )
            .await;

            persist_user_shell_output(
                &session,
                turn_context.as_ref(),
                &plan.raw_command,
                &output,
                mode,
            )
            .await;
        }
        Ok(Err(err)) => {
            error!("user shell command failed: {err:?}");
            let message = format!("execution error: {err:?}");
            let exec_output = failed_output(message);
            let formatted_output =
                format_exec_output_str(&exec_output, turn_context.truncation_policy);
            events::send_exec_end(
                &session,
                turn_context.as_ref(),
                &plan,
                &exec_output,
                formatted_output,
                ExecCommandStatus::Failed,
            )
            .await;
            persist_user_shell_output(
                &session,
                turn_context.as_ref(),
                &plan.raw_command,
                &exec_output,
                mode,
            )
            .await;
        }
    }
}
