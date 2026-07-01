use super::automation_schedule::next_automation_run_at;
use super::*;
use crate::automation_projection::api_automation_run_from_state;
use praxis_app_gateway_protocol::AutomationRunUpdatedNotification;
use praxis_protocol::user_input::UserInput as CoreUserInput;
use praxis_state::Automation as StateAutomation;
use praxis_state::AutomationRunCreateParams as StateAutomationRunCreateParams;
use praxis_state::AutomationRunStatus as StateAutomationRunStatus;
use praxis_state::AutomationRunTrigger as StateAutomationRunTrigger;
use serde_json::json;

const AUTOMATION_SCHEDULER_TICK: Duration = Duration::from_secs(5);
const AUTOMATION_DUE_BATCH_LIMIT: u32 = 16;

impl PraxisMessageProcessor {
    pub(crate) fn start_automation_scheduler(&self) {
        let tracker = self.background_tasks.clone();
        let config = self.config.clone();
        let thread_manager = self.thread_manager.clone();
        let outgoing = self.outgoing.clone();
        self.background_tasks.spawn(async move {
            let mut stale_runs_failed = false;
            loop {
                if tracker.is_closed() {
                    break;
                }
                let Some(state_db) = get_state_db(config.as_ref()).await else {
                    tokio::time::sleep(AUTOMATION_SCHEDULER_TICK).await;
                    continue;
                };
                if !stale_runs_failed {
                    stale_runs_failed = true;
                    match state_db
                        .fail_stale_automation_runs(
                            "gateway restarted before automation run completed",
                        )
                        .await
                    {
                        Ok(runs) => {
                            for run in runs {
                                emit_automation_run_updated(outgoing.as_ref(), run).await;
                            }
                        }
                        Err(err) => warn!("failed to clear stale automation runs: {err}"),
                    }
                }
                if let Err(err) = run_due_automations(
                    state_db.as_ref(),
                    thread_manager.as_ref(),
                    outgoing.as_ref(),
                )
                .await
                {
                    warn!("automation scheduler tick failed: {err}");
                }
                tokio::time::sleep(AUTOMATION_SCHEDULER_TICK).await;
            }
        });
    }
}

async fn run_due_automations(
    state_db: &StateRuntime,
    thread_manager: &ThreadManager,
    outgoing: &OutgoingMessageSender,
) -> anyhow::Result<()> {
    let now = Utc::now();
    let automations = state_db
        .list_due_automations(now, AUTOMATION_DUE_BATCH_LIMIT)
        .await?;
    for automation in automations {
        let next_run_at =
            match next_automation_run_at(automation.kind, &automation.schedule_json, now) {
                Ok(next_run_at) => next_run_at,
                Err(message) => {
                    let _ = state_db
                        .update_automation_schedule_mark(automation.automation_id.as_str(), None)
                        .await;
                    let run =
                        create_failed_scheduled_run(state_db, &automation, None, message.as_str())
                            .await?;
                    emit_automation_run_updated(outgoing, run).await;
                    continue;
                }
            };
        let _ = state_db
            .update_automation_schedule_mark(automation.automation_id.as_str(), next_run_at)
            .await;
        let Some(thread_id) = automation.thread_id.clone() else {
            let run = create_failed_scheduled_run(
                state_db,
                &automation,
                None,
                "scheduled automation has no target threadId",
            )
            .await?;
            emit_automation_run_updated(outgoing, run).await;
            continue;
        };
        let parsed_thread_id = match ThreadId::from_string(thread_id.as_str()) {
            Ok(thread_id) => thread_id,
            Err(err) => {
                let message = format!("invalid target threadId: {err}");
                let run = create_failed_scheduled_run(
                    state_db,
                    &automation,
                    Some(thread_id.as_str()),
                    message.as_str(),
                )
                .await?;
                emit_automation_run_updated(outgoing, run).await;
                continue;
            }
        };
        let Some(run) = state_db
            .create_automation_run(&StateAutomationRunCreateParams {
                automation_id: automation.automation_id.clone(),
                status: StateAutomationRunStatus::Queued,
                trigger: StateAutomationRunTrigger::Scheduled,
                thread_id: Some(thread_id.clone()),
                turn_id: None,
                metadata_json: scheduled_run_metadata(&automation),
            })
            .await?
        else {
            continue;
        };
        let thread = match thread_manager.get_thread(parsed_thread_id).await {
            Ok(thread) => thread,
            Err(err) => {
                let message = format!("target thread is not loaded: {err}");
                let run = state_db
                    .finish_automation_run(
                        run.run_id.as_str(),
                        StateAutomationRunStatus::Failed,
                        Some(message.as_str()),
                    )
                    .await?
                    .unwrap_or(run);
                emit_automation_run_updated(outgoing, run).await;
                continue;
            }
        };
        let turn_id = match thread
            .submit_user_turn(
                vec![CoreUserInput::Text {
                    text: automation.prompt.clone(),
                    text_elements: Vec::new(),
                }],
                automation.config_json.get("outputSchema").cloned(),
            )
            .await
        {
            Ok(turn_id) => turn_id,
            Err(err) => {
                let message = format!("failed to start automation turn: {err}");
                let run = state_db
                    .finish_automation_run(
                        run.run_id.as_str(),
                        StateAutomationRunStatus::Failed,
                        Some(message.as_str()),
                    )
                    .await?
                    .unwrap_or(run);
                emit_automation_run_updated(outgoing, run).await;
                continue;
            }
        };
        let Some(run) = state_db
            .mark_automation_run_running(run.run_id.as_str(), thread_id.as_str(), turn_id.as_str())
            .await?
        else {
            continue;
        };
        emit_automation_run_updated(outgoing, run).await;
    }
    Ok(())
}

async fn create_failed_scheduled_run(
    state_db: &StateRuntime,
    automation: &StateAutomation,
    thread_id: Option<&str>,
    message: &str,
) -> anyhow::Result<praxis_state::AutomationRun> {
    let run = state_db
        .create_automation_run(&StateAutomationRunCreateParams {
            automation_id: automation.automation_id.clone(),
            status: StateAutomationRunStatus::Queued,
            trigger: StateAutomationRunTrigger::Scheduled,
            thread_id: thread_id.map(str::to_string),
            turn_id: None,
            metadata_json: scheduled_run_metadata(automation),
        })
        .await?
        .ok_or_else(|| anyhow::anyhow!("automation disappeared: {}", automation.automation_id))?;
    state_db
        .finish_automation_run(
            run.run_id.as_str(),
            StateAutomationRunStatus::Failed,
            Some(message),
        )
        .await?
        .ok_or_else(|| anyhow::anyhow!("automation run disappeared: {}", run.run_id))
}

fn scheduled_run_metadata(automation: &StateAutomation) -> serde_json::Value {
    json!({
        "trigger": "scheduled",
        "automationKind": automation.kind.as_str(),
    })
}

async fn emit_automation_run_updated(
    outgoing: &OutgoingMessageSender,
    run: praxis_state::AutomationRun,
) {
    outgoing
        .send_server_notification(ServerNotification::AutomationRunUpdated(
            AutomationRunUpdatedNotification {
                run: api_automation_run_from_state(run),
            },
        ))
        .await;
}
