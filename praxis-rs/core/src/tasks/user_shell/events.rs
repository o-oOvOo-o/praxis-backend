use crate::exec::ExecToolCallOutput;
use crate::praxis::Session;
use crate::praxis::TurnContext;
use praxis_protocol::protocol::EventMsg;
use praxis_protocol::protocol::ExecCommandBeginEvent;
use praxis_protocol::protocol::ExecCommandEndEvent;
use praxis_protocol::protocol::ExecCommandSource;
use praxis_protocol::protocol::ExecCommandStatus;
use praxis_protocol::protocol::TurnStartedEvent;

use super::execution_plan::UserShellExecutionPlan;
use super::types::UserShellCommandMode;

pub(super) async fn send_turn_started_if_needed(
    session: &Session,
    turn_context: &TurnContext,
    mode: UserShellCommandMode,
) {
    if mode != UserShellCommandMode::StandaloneTurn {
        return;
    }

    let event = EventMsg::TurnStarted(TurnStartedEvent {
        turn_id: turn_context.sub_id.clone(),
        model_context_window: turn_context.model_context_window(),
        collaboration_mode_kind: turn_context.collaboration_mode.mode,
    });
    session.send_event(turn_context, event).await;
}

pub(super) async fn send_exec_begin(
    session: &Session,
    turn_context: &TurnContext,
    plan: &UserShellExecutionPlan,
) {
    session
        .send_event(
            turn_context,
            EventMsg::ExecCommandBegin(ExecCommandBeginEvent {
                call_id: plan.call_id.clone(),
                process_id: None,
                turn_id: turn_context.sub_id.clone(),
                command: plan.display_command.clone(),
                cwd: plan.cwd.clone(),
                parsed_cmd: plan.parsed_cmd.clone(),
                source: ExecCommandSource::UserShell,
                interaction_input: None,
            }),
        )
        .await;
}

pub(super) async fn send_exec_end(
    session: &Session,
    turn_context: &TurnContext,
    plan: &UserShellExecutionPlan,
    exec_output: &ExecToolCallOutput,
    formatted_output: String,
    status: ExecCommandStatus,
) {
    session
        .send_event(
            turn_context,
            EventMsg::ExecCommandEnd(ExecCommandEndEvent {
                call_id: plan.call_id.clone(),
                process_id: None,
                turn_id: turn_context.sub_id.clone(),
                command: plan.display_command.clone(),
                cwd: plan.cwd.clone(),
                parsed_cmd: plan.parsed_cmd.clone(),
                source: ExecCommandSource::UserShell,
                interaction_input: None,
                stdout: exec_output.stdout.text.clone(),
                stderr: exec_output.stderr.text.clone(),
                aggregated_output: exec_output.aggregated_output.text.clone(),
                exit_code: exec_output.exit_code,
                duration: exec_output.duration,
                formatted_output,
                status,
            }),
        )
        .await;
}
