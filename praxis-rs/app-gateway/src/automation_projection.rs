use praxis_app_gateway_protocol::AutomationRun as ApiAutomationRun;
use praxis_app_gateway_protocol::AutomationRunStatus as ApiAutomationRunStatus;
use praxis_app_gateway_protocol::AutomationRunTrigger as ApiAutomationRunTrigger;
use praxis_state::AutomationRun as StateAutomationRun;
use praxis_state::AutomationRunStatus as StateAutomationRunStatus;
use praxis_state::AutomationRunTrigger as StateAutomationRunTrigger;

pub(crate) fn api_automation_run_from_state(run: StateAutomationRun) -> ApiAutomationRun {
    ApiAutomationRun {
        run_id: run.run_id,
        automation_id: run.automation_id,
        status: api_automation_run_status_from_state(run.status),
        trigger: api_automation_run_trigger_from_state(run.trigger),
        thread_id: run.thread_id,
        turn_id: run.turn_id,
        started_at_ms: run.started_at.timestamp_millis(),
        completed_at_ms: run.completed_at.map(|value| value.timestamp_millis()),
        error: run.error,
        metadata: run.metadata_json,
    }
}

fn api_automation_run_status_from_state(
    status: StateAutomationRunStatus,
) -> ApiAutomationRunStatus {
    match status {
        StateAutomationRunStatus::Queued => ApiAutomationRunStatus::Queued,
        StateAutomationRunStatus::Running => ApiAutomationRunStatus::Running,
        StateAutomationRunStatus::Succeeded => ApiAutomationRunStatus::Succeeded,
        StateAutomationRunStatus::Failed => ApiAutomationRunStatus::Failed,
        StateAutomationRunStatus::Cancelled => ApiAutomationRunStatus::Cancelled,
    }
}

fn api_automation_run_trigger_from_state(
    trigger: StateAutomationRunTrigger,
) -> ApiAutomationRunTrigger {
    match trigger {
        StateAutomationRunTrigger::Manual => ApiAutomationRunTrigger::Manual,
        StateAutomationRunTrigger::Scheduled => ApiAutomationRunTrigger::Scheduled,
    }
}
