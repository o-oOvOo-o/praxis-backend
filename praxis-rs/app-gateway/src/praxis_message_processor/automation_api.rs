use super::*;
use crate::automation_projection::api_automation_run_from_state;
use chrono::DateTime;
use chrono::TimeZone;
use chrono::Utc;
use praxis_app_gateway_protocol::Automation as ApiAutomation;
use praxis_app_gateway_protocol::AutomationCreateParams as ApiAutomationCreateParams;
use praxis_app_gateway_protocol::AutomationCreateResponse;
use praxis_app_gateway_protocol::AutomationDeleteParams as ApiAutomationDeleteParams;
use praxis_app_gateway_protocol::AutomationDeleteResponse;
use praxis_app_gateway_protocol::AutomationGetParams as ApiAutomationGetParams;
use praxis_app_gateway_protocol::AutomationGetResponse;
use praxis_app_gateway_protocol::AutomationHistoryParams as ApiAutomationHistoryParams;
use praxis_app_gateway_protocol::AutomationHistoryResponse;
use praxis_app_gateway_protocol::AutomationKind as ApiAutomationKind;
use praxis_app_gateway_protocol::AutomationListParams as ApiAutomationListParams;
use praxis_app_gateway_protocol::AutomationListResponse;
use praxis_app_gateway_protocol::AutomationRunNowParams as ApiAutomationRunNowParams;
use praxis_app_gateway_protocol::AutomationRunNowResponse;
use praxis_app_gateway_protocol::AutomationRunUpdatedNotification;
use praxis_app_gateway_protocol::AutomationUpdateParams as ApiAutomationUpdateParams;
use praxis_app_gateway_protocol::AutomationUpdateResponse;
use praxis_app_gateway_protocol::Turn;
use praxis_app_gateway_protocol::TurnStatus;
use praxis_protocol::user_input::UserInput as CoreUserInput;
use praxis_state::Automation as StateAutomation;
use praxis_state::AutomationCreateParams as StateAutomationCreateParams;
use praxis_state::AutomationKind as StateAutomationKind;
use praxis_state::AutomationRun as StateAutomationRun;
use praxis_state::AutomationRunCreateParams as StateAutomationRunCreateParams;
use praxis_state::AutomationRunStatus as StateAutomationRunStatus;
use praxis_state::AutomationRunTrigger as StateAutomationRunTrigger;
use praxis_state::AutomationUpdate as StateAutomationUpdate;
use serde_json::Value as JsonValue;

impl PraxisMessageProcessor {
    pub(crate) async fn automation_list(
        &self,
        request_id: ConnectionRequestId,
        params: ApiAutomationListParams,
    ) {
        let Some(state_db) = self.automation_state_db(request_id.clone()).await else {
            return;
        };
        match state_db.list_automations(params.include_disabled).await {
            Ok(items) => {
                self.outgoing
                    .send_response(
                        request_id,
                        AutomationListResponse {
                            data: items.into_iter().map(api_automation_from_state).collect(),
                        },
                    )
                    .await;
            }
            Err(err) => {
                self.send_internal_error(request_id, format!("failed to list automations: {err}"))
                    .await;
            }
        }
    }

    pub(crate) async fn automation_get(
        &self,
        request_id: ConnectionRequestId,
        params: ApiAutomationGetParams,
    ) {
        if params.automation_id.trim().is_empty() {
            self.send_invalid_request_error(
                request_id,
                "automationId must not be empty".to_string(),
            )
            .await;
            return;
        }
        let Some(state_db) = self.automation_state_db(request_id.clone()).await else {
            return;
        };
        match state_db.get_automation(params.automation_id.as_str()).await {
            Ok(automation) => {
                self.outgoing
                    .send_response(
                        request_id,
                        AutomationGetResponse {
                            automation: automation.map(api_automation_from_state),
                        },
                    )
                    .await;
            }
            Err(err) => {
                self.send_internal_error(request_id, format!("failed to read automation: {err}"))
                    .await;
            }
        }
    }

    pub(crate) async fn automation_create(
        &self,
        request_id: ConnectionRequestId,
        params: ApiAutomationCreateParams,
    ) {
        let params = match normalize_create_params(params) {
            Ok(params) => params,
            Err(message) => {
                self.send_invalid_request_error(request_id, message).await;
                return;
            }
        };
        let Some(state_db) = self.automation_state_db(request_id.clone()).await else {
            return;
        };
        match state_db.create_automation(&params).await {
            Ok(automation) => {
                self.outgoing
                    .send_response(
                        request_id,
                        AutomationCreateResponse {
                            automation: api_automation_from_state(automation),
                        },
                    )
                    .await;
            }
            Err(err) => {
                self.send_internal_error(request_id, format!("failed to create automation: {err}"))
                    .await;
            }
        }
    }

    pub(crate) async fn automation_update(
        &self,
        request_id: ConnectionRequestId,
        params: ApiAutomationUpdateParams,
    ) {
        if params.automation_id.trim().is_empty() {
            self.send_invalid_request_error(
                request_id,
                "automationId must not be empty".to_string(),
            )
            .await;
            return;
        }
        let Some(state_db) = self.automation_state_db(request_id.clone()).await else {
            return;
        };
        let existing = match state_db.get_automation(params.automation_id.as_str()).await {
            Ok(Some(existing)) => existing,
            Ok(None) => {
                self.send_invalid_request_error(
                    request_id,
                    format!("automation not found: {}", params.automation_id),
                )
                .await;
                return;
            }
            Err(err) => {
                self.send_internal_error(request_id, format!("failed to read automation: {err}"))
                    .await;
                return;
            }
        };
        let update = match normalize_update_params(&existing, params) {
            Ok(update) => update,
            Err(message) => {
                self.send_invalid_request_error(request_id, message).await;
                return;
            }
        };
        match state_db
            .update_automation(existing.automation_id.as_str(), update)
            .await
        {
            Ok(Some(automation)) => {
                self.outgoing
                    .send_response(
                        request_id,
                        AutomationUpdateResponse {
                            automation: api_automation_from_state(automation),
                        },
                    )
                    .await;
            }
            Ok(None) => {
                self.send_invalid_request_error(
                    request_id,
                    format!("automation not found: {}", existing.automation_id),
                )
                .await;
            }
            Err(err) => {
                self.send_internal_error(request_id, format!("failed to update automation: {err}"))
                    .await;
            }
        }
    }

    pub(crate) async fn automation_delete(
        &self,
        request_id: ConnectionRequestId,
        params: ApiAutomationDeleteParams,
    ) {
        if params.automation_id.trim().is_empty() {
            self.send_invalid_request_error(
                request_id,
                "automationId must not be empty".to_string(),
            )
            .await;
            return;
        }
        let Some(state_db) = self.automation_state_db(request_id.clone()).await else {
            return;
        };
        match state_db
            .delete_automation(params.automation_id.as_str())
            .await
        {
            Ok(deleted) => {
                self.outgoing
                    .send_response(request_id, AutomationDeleteResponse { deleted })
                    .await;
            }
            Err(err) => {
                self.send_internal_error(request_id, format!("failed to delete automation: {err}"))
                    .await;
            }
        }
    }

    pub(crate) async fn automation_history(
        &self,
        request_id: ConnectionRequestId,
        params: ApiAutomationHistoryParams,
    ) {
        if params.automation_id.trim().is_empty() {
            self.send_invalid_request_error(
                request_id,
                "automationId must not be empty".to_string(),
            )
            .await;
            return;
        }
        let Some(state_db) = self.automation_state_db(request_id.clone()).await else {
            return;
        };
        match state_db.get_automation(params.automation_id.as_str()).await {
            Ok(Some(_)) => {}
            Ok(None) => {
                self.send_invalid_request_error(
                    request_id,
                    format!("automation not found: {}", params.automation_id),
                )
                .await;
                return;
            }
            Err(err) => {
                self.send_internal_error(request_id, format!("failed to read automation: {err}"))
                    .await;
                return;
            }
        }
        match state_db
            .list_automation_runs(params.automation_id.as_str(), params.limit)
            .await
        {
            Ok(runs) => {
                self.outgoing
                    .send_response(
                        request_id,
                        AutomationHistoryResponse {
                            data: runs
                                .into_iter()
                                .map(api_automation_run_from_state)
                                .collect(),
                        },
                    )
                    .await;
            }
            Err(err) => {
                self.send_internal_error(
                    request_id,
                    format!("failed to list automation history: {err}"),
                )
                .await;
            }
        }
    }

    pub(crate) async fn automation_run_now(
        &self,
        request_id: ConnectionRequestId,
        params: ApiAutomationRunNowParams,
    ) {
        if params.automation_id.trim().is_empty() {
            self.send_invalid_request_error(
                request_id,
                "automationId must not be empty".to_string(),
            )
            .await;
            return;
        }
        if params.thread_id.trim().is_empty() {
            self.send_invalid_request_error(request_id, "threadId must not be empty".to_string())
                .await;
            return;
        }
        if !params.metadata.is_object() {
            self.send_invalid_request_error(
                request_id,
                "metadata must be a JSON object".to_string(),
            )
            .await;
            return;
        }
        let Some(state_db) = self.automation_state_db(request_id.clone()).await else {
            return;
        };
        let automation = match state_db.get_automation(params.automation_id.as_str()).await {
            Ok(Some(automation)) => automation,
            Ok(None) => {
                self.send_invalid_request_error(
                    request_id,
                    format!("automation not found: {}", params.automation_id),
                )
                .await;
                return;
            }
            Err(err) => {
                self.send_internal_error(request_id, format!("failed to read automation: {err}"))
                    .await;
                return;
            }
        };
        let Some((_, thread)) = self
            .ensure_thread_for_request(params.thread_id.as_str(), &request_id)
            .await
        else {
            return;
        };
        let run = match state_db
            .create_automation_run(&StateAutomationRunCreateParams {
                automation_id: automation.automation_id.clone(),
                status: StateAutomationRunStatus::Queued,
                trigger: StateAutomationRunTrigger::Manual,
                thread_id: Some(params.thread_id.clone()),
                turn_id: None,
                metadata_json: run_now_metadata(params.metadata, &automation),
            })
            .await
        {
            Ok(Some(run)) => run,
            Ok(None) => {
                self.send_invalid_request_error(
                    request_id,
                    format!("automation not found: {}", automation.automation_id),
                )
                .await;
                return;
            }
            Err(err) => {
                self.send_internal_error(
                    request_id,
                    format!("failed to create automation run: {err}"),
                )
                .await;
                return;
            }
        };

        let turn_id = self
            .submit_core_op(
                &request_id,
                thread.as_ref(),
                thread.config_snapshot().await.user_turn_op(
                    vec![CoreUserInput::Text {
                        text: automation.prompt.clone(),
                        text_elements: Vec::new(),
                    }],
                    automation.config_json.get("outputSchema").cloned(),
                ),
            )
            .await;

        let turn_id = match turn_id {
            Ok(turn_id) => turn_id,
            Err(err) => {
                let message = format!("failed to start automation turn: {err}");
                if let Ok(Some(run)) = state_db
                    .finish_automation_run(
                        run.run_id.as_str(),
                        StateAutomationRunStatus::Failed,
                        Some(message.as_str()),
                    )
                    .await
                {
                    self.emit_automation_run_updated(run).await;
                }
                self.send_internal_error(request_id, message).await;
                return;
            }
        };

        self.outgoing
            .record_request_turn_id(&request_id, turn_id.as_str())
            .await;
        let run = match state_db
            .mark_automation_run_running(
                run.run_id.as_str(),
                params.thread_id.as_str(),
                turn_id.as_str(),
            )
            .await
        {
            Ok(Some(run)) => run,
            Ok(None) => {
                self.send_internal_error(
                    request_id,
                    format!("automation run disappeared before start: {}", run.run_id),
                )
                .await;
                return;
            }
            Err(err) => {
                self.send_internal_error(
                    request_id,
                    format!("failed to mark automation run running: {err}"),
                )
                .await;
                return;
            }
        };
        self.emit_automation_run_updated(run.clone()).await;
        let turn = Turn {
            id: turn_id,
            items: Vec::new(),
            error: None,
            status: TurnStatus::InProgress,
        };
        self.outgoing
            .send_response(
                request_id,
                AutomationRunNowResponse {
                    run: api_automation_run_from_state(run),
                    turn,
                },
            )
            .await;
    }

    async fn automation_state_db(
        &self,
        request_id: ConnectionRequestId,
    ) -> Option<Arc<StateRuntime>> {
        match get_state_db(self.config.as_ref()).await {
            Some(state_db) => Some(state_db),
            None => {
                self.send_internal_error(request_id, "state database is unavailable".to_string())
                    .await;
                None
            }
        }
    }

    pub(crate) async fn emit_automation_run_updated(&self, run: StateAutomationRun) {
        self.outgoing
            .send_server_notification(ServerNotification::AutomationRunUpdated(
                AutomationRunUpdatedNotification {
                    run: api_automation_run_from_state(run),
                },
            ))
            .await;
    }
}

fn api_automation_from_state(value: StateAutomation) -> ApiAutomation {
    ApiAutomation {
        automation_id: value.automation_id,
        name: value.name,
        enabled: value.enabled,
        kind: api_automation_kind_from_state(value.kind),
        prompt: value.prompt,
        thread_id: value.thread_id,
        schedule: value.schedule_json,
        config: value.config_json,
        next_run_at_ms: value.next_run_at.map(|value| value.timestamp_millis()),
        last_run_at_ms: value.last_run_at.map(|value| value.timestamp_millis()),
        created_at_ms: value.created_at.timestamp_millis(),
        updated_at_ms: value.updated_at.timestamp_millis(),
    }
}

fn api_automation_kind_from_state(value: StateAutomationKind) -> ApiAutomationKind {
    match value {
        StateAutomationKind::Heartbeat => ApiAutomationKind::Heartbeat,
        StateAutomationKind::Cron => ApiAutomationKind::Cron,
    }
}

fn state_automation_kind_from_api(value: ApiAutomationKind) -> StateAutomationKind {
    match value {
        ApiAutomationKind::Heartbeat => StateAutomationKind::Heartbeat,
        ApiAutomationKind::Cron => StateAutomationKind::Cron,
    }
}

fn normalize_create_params(
    params: ApiAutomationCreateParams,
) -> Result<StateAutomationCreateParams, String> {
    let name = non_empty("name", params.name)?;
    let prompt = non_empty("prompt", params.prompt)?;
    let thread_id = params
        .thread_id
        .map(|thread_id| non_empty("threadId", thread_id))
        .transpose()?;
    let kind = state_automation_kind_from_api(params.kind);
    validate_schedule(kind, &params.schedule)?;
    validate_object("config", &params.config)?;
    Ok(StateAutomationCreateParams {
        name,
        enabled: params.enabled.unwrap_or(true),
        kind,
        thread_id,
        prompt,
        schedule_json: params.schedule,
        config_json: params.config,
        next_run_at: params.next_run_at_ms.map(millis_to_datetime).transpose()?,
    })
}

fn normalize_update_params(
    existing: &StateAutomation,
    params: ApiAutomationUpdateParams,
) -> Result<StateAutomationUpdate, String> {
    if params.clear_next_run_at && params.next_run_at_ms.is_some() {
        return Err("nextRunAtMs and clearNextRunAt cannot both be set".to_string());
    }
    if params.clear_thread_id && params.thread_id.is_some() {
        return Err("threadId and clearThreadId cannot both be set".to_string());
    }
    let kind = params
        .kind
        .map(state_automation_kind_from_api)
        .unwrap_or(existing.kind);
    let prompt = params.prompt.as_deref().unwrap_or(existing.prompt.as_str());
    if prompt.trim().is_empty() {
        return Err("prompt must not be empty".to_string());
    }
    let schedule = params.schedule.as_ref().unwrap_or(&existing.schedule_json);
    validate_schedule(kind, schedule)?;
    let config = params.config.as_ref().unwrap_or(&existing.config_json);
    validate_object("config", config)?;
    Ok(StateAutomationUpdate {
        name: params
            .name
            .map(|name| non_empty("name", name))
            .transpose()?,
        enabled: params.enabled,
        kind: params.kind.map(state_automation_kind_from_api),
        thread_id: if params.clear_thread_id {
            Some(None)
        } else {
            params
                .thread_id
                .map(|thread_id| non_empty("threadId", thread_id).map(Some))
                .transpose()?
        },
        prompt: params
            .prompt
            .map(|prompt| non_empty("prompt", prompt))
            .transpose()?,
        schedule_json: params.schedule,
        config_json: params.config,
        next_run_at: if params.clear_next_run_at {
            Some(None)
        } else {
            params
                .next_run_at_ms
                .map(|value| millis_to_datetime(value).map(Some))
                .transpose()?
        },
    })
}

fn non_empty(field: &str, value: String) -> Result<String, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(format!("{field} must not be empty"));
    }
    Ok(trimmed.to_string())
}

fn validate_object(field: &str, value: &JsonValue) -> Result<(), String> {
    if value.is_object() {
        Ok(())
    } else {
        Err(format!("{field} must be a JSON object"))
    }
}

fn validate_schedule(kind: StateAutomationKind, value: &JsonValue) -> Result<(), String> {
    validate_object("schedule", value)?;
    match kind {
        StateAutomationKind::Heartbeat => {
            let interval_ms = value
                .get("intervalMs")
                .and_then(JsonValue::as_i64)
                .ok_or_else(|| "heartbeat schedule requires intervalMs".to_string())?;
            if interval_ms <= 0 {
                return Err("heartbeat schedule intervalMs must be greater than zero".to_string());
            }
        }
        StateAutomationKind::Cron => {
            let cron = value
                .get("cron")
                .and_then(JsonValue::as_str)
                .map(str::trim)
                .unwrap_or_default();
            if cron.is_empty() {
                return Err("cron schedule requires cron".to_string());
            }
        }
    }
    Ok(())
}

fn millis_to_datetime(value: i64) -> Result<DateTime<Utc>, String> {
    Utc.timestamp_millis_opt(value)
        .single()
        .ok_or_else(|| format!("invalid unix millis timestamp `{value}`"))
}

fn run_now_metadata(mut metadata: JsonValue, automation: &StateAutomation) -> JsonValue {
    if let Some(object) = metadata.as_object_mut() {
        object.insert(
            "trigger".to_string(),
            JsonValue::String("manual".to_string()),
        );
        object.insert(
            "automationKind".to_string(),
            JsonValue::String(automation.kind.as_str().to_string()),
        );
    }
    metadata
}
