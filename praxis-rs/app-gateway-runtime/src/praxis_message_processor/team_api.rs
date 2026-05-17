use std::sync::Arc;

use praxis_app_gateway_protocol::ServerNotification;
use praxis_app_gateway_protocol::Team;
use praxis_app_gateway_protocol::TeamCreateParams;
use praxis_app_gateway_protocol::TeamCreateResponse;
use praxis_app_gateway_protocol::TeamDeleteParams;
use praxis_app_gateway_protocol::TeamDeleteResponse;
use praxis_app_gateway_protocol::TeamDeletedNotification;
use praxis_app_gateway_protocol::TeamExecutionMode;
use praxis_app_gateway_protocol::TeamMailboxMessage;
use praxis_app_gateway_protocol::TeamMailboxParticipant;
use praxis_app_gateway_protocol::TeamMailboxUpdatedNotification;
use praxis_app_gateway_protocol::TeamReadParams;
use praxis_app_gateway_protocol::TeamReadResponse;
use praxis_app_gateway_protocol::TeamResumeMode;
use praxis_app_gateway_protocol::TeamTask;
use praxis_app_gateway_protocol::TeamTaskCreateParams;
use praxis_app_gateway_protocol::TeamTaskCreateResponse;
use praxis_app_gateway_protocol::TeamTaskListParams;
use praxis_app_gateway_protocol::TeamTaskListResponse;
use praxis_app_gateway_protocol::TeamTaskStatus;
use praxis_app_gateway_protocol::TeamTaskUpdateParams;
use praxis_app_gateway_protocol::TeamTaskUpdateResponse;
use praxis_app_gateway_protocol::TeamTaskUpdatedNotification;
use praxis_app_gateway_protocol::TeamTeammate;
use praxis_app_gateway_protocol::TeamTeammateCreateParams;
use praxis_app_gateway_protocol::TeamTeammateCreateResponse;
use praxis_app_gateway_protocol::TeamTeammateMessageParams;
use praxis_app_gateway_protocol::TeamTeammateMessageResponse;
use praxis_app_gateway_protocol::TeamTeammateStatus;
use praxis_app_gateway_protocol::TeamTeammateUpdatedNotification;
use praxis_app_gateway_protocol::TeamUpdatedNotification;
use praxis_app_gateway_protocol::ThreadStartedNotification;
use praxis_core::NewThread;
use praxis_core::config::Config;
use praxis_protocol::AgentPath;
use praxis_protocol::ThreadId;
use praxis_protocol::dynamic_tools::DynamicToolSpec as CoreDynamicToolSpec;
use praxis_protocol::protocol::InterAgentCommunication;
use praxis_protocol::protocol::Op;
use praxis_protocol::protocol::SessionSource;
use praxis_protocol::protocol::SubAgentSource;
use praxis_rollout::state_db::StateDbHandle;
use praxis_rollout::state_db::get_state_db;
use tracing::warn;
use uuid::Uuid;

use super::PraxisMessageProcessor;
use super::build_thread_from_snapshot;
use super::config_load_error;
use super::derive_config_from_params;
use crate::outgoing_message::ConnectionRequestId;
use crate::outgoing_message::OutgoingMessageSender;
use crate::thread_status::resolve_thread_status;

#[derive(Default)]
struct TeamLeadDefaults {
    model: Option<String>,
    model_provider: Option<String>,
    cwd: Option<String>,
}

struct TeamLeadSpawnContext {
    parent_thread_id: ThreadId,
    parent_depth: i32,
    parent_agent_path: AgentPath,
}

impl PraxisMessageProcessor {
    pub(super) async fn team_create(
        &self,
        request_id: ConnectionRequestId,
        params: TeamCreateParams,
    ) {
        let Some(state_db_ctx) = self.require_team_state_db(&request_id).await else {
            return;
        };
        let Some(_) = self
            .require_known_thread_reference(&request_id, &params.lead_thread_id)
            .await
        else {
            return;
        };
        let Some(name) = normalize_required_text(params.name.as_str()) else {
            self.send_invalid_request_error(request_id, "team name must not be empty".to_string())
                .await;
            return;
        };
        let objective = normalize_optional_text(params.objective);
        let team_id = params.team_id.unwrap_or_else(new_team_id);
        match state_db_ctx.get_team(team_id.as_str()).await {
            Ok(Some(_)) => {
                self.send_invalid_request_error(
                    request_id,
                    format!("team already exists: {team_id}"),
                )
                .await;
                return;
            }
            Ok(None) => {}
            Err(err) => {
                self.send_internal_error(
                    request_id,
                    format!("failed to check existing team {team_id}: {err}"),
                )
                .await;
                return;
            }
        }

        let team = match state_db_ctx
            .create_team(&praxis_state::TeamCreateParams {
                id: team_id,
                lead_thread_id: params.lead_thread_id,
                name,
                objective,
                execution_mode: praxis_state::TeamExecutionMode::ProcessFirst,
                resume_mode: praxis_state::TeamResumeMode::Strong,
            })
            .await
        {
            Ok(team) => team,
            Err(err) => {
                self.send_internal_error(request_id, format!("failed to create team: {err}"))
                    .await;
                return;
            }
        };

        let team = api_team_from_state(&team);
        self.outgoing
            .send_response(request_id, TeamCreateResponse { team: team.clone() })
            .await;
        self.outgoing
            .send_server_notification(ServerNotification::TeamUpdated(TeamUpdatedNotification {
                team,
            }))
            .await;
    }

    pub(super) async fn team_read(&self, request_id: ConnectionRequestId, params: TeamReadParams) {
        let Some(state_db_ctx) = self.require_team_state_db(&request_id).await else {
            return;
        };
        let Some(team) = self
            .load_team_or_send_invalid(&request_id, &state_db_ctx, params.team_id.as_str())
            .await
        else {
            return;
        };

        let teammates = if params.include_teammates {
            match state_db_ctx.list_team_teammates(team.id.as_str()).await {
                Ok(teammates) => teammates
                    .iter()
                    .map(api_team_teammate_from_state)
                    .collect::<Vec<_>>(),
                Err(err) => {
                    self.send_internal_error(
                        request_id,
                        format!("failed to list teammates for team {}: {err}", team.id),
                    )
                    .await;
                    return;
                }
            }
        } else {
            Vec::new()
        };

        let tasks = if params.include_tasks {
            match state_db_ctx.list_team_tasks(team.id.as_str()).await {
                Ok(tasks) => tasks
                    .iter()
                    .map(api_team_task_from_state)
                    .collect::<Vec<_>>(),
                Err(err) => {
                    self.send_internal_error(
                        request_id,
                        format!("failed to list tasks for team {}: {err}", team.id),
                    )
                    .await;
                    return;
                }
            }
        } else {
            Vec::new()
        };

        let messages = if params.include_messages {
            match state_db_ctx
                .list_team_mailbox_messages(
                    team.id.as_str(),
                    params.message_limit.map(|value| value as usize),
                )
                .await
            {
                Ok(messages) => messages
                    .iter()
                    .map(api_team_mailbox_message_from_state)
                    .collect::<Vec<_>>(),
                Err(err) => {
                    self.send_internal_error(
                        request_id,
                        format!(
                            "failed to list mailbox messages for team {}: {err}",
                            team.id
                        ),
                    )
                    .await;
                    return;
                }
            }
        } else {
            Vec::new()
        };

        self.outgoing
            .send_response(
                request_id,
                TeamReadResponse {
                    team: api_team_from_state(&team),
                    teammates,
                    tasks,
                    messages,
                },
            )
            .await;
    }

    pub(super) async fn team_delete(
        &self,
        request_id: ConnectionRequestId,
        params: TeamDeleteParams,
    ) {
        let Some(state_db_ctx) = self.require_team_state_db(&request_id).await else {
            return;
        };
        let Some(team) = self
            .load_team_or_send_invalid(&request_id, &state_db_ctx, params.team_id.as_str())
            .await
        else {
            return;
        };

        match state_db_ctx.delete_team(team.id.as_str()).await {
            Ok(_) => {
                self.outgoing
                    .send_response(request_id, TeamDeleteResponse {})
                    .await;
                self.outgoing
                    .send_server_notification(ServerNotification::TeamDeleted(
                        TeamDeletedNotification { team_id: team.id },
                    ))
                    .await;
            }
            Err(err) => {
                self.send_internal_error(request_id, format!("failed to delete team: {err}"))
                    .await;
            }
        }
    }

    pub(super) async fn team_teammate_create(
        &mut self,
        request_id: ConnectionRequestId,
        params: TeamTeammateCreateParams,
    ) {
        let Some(state_db_ctx) = self.require_team_state_db(&request_id).await else {
            return;
        };
        let Some(team_state) = self
            .load_team_or_send_invalid(&request_id, &state_db_ctx, params.team_id.as_str())
            .await
        else {
            return;
        };

        let Some(name) = normalize_required_text(params.name.as_str()) else {
            self.send_invalid_request_error(
                request_id,
                "teammate name must not be empty".to_string(),
            )
            .await;
            return;
        };
        let role = normalize_optional_text(params.role);
        let teammate_id = match params.teammate_id {
            Some(teammate_id) => {
                let Some(teammate_id) = normalize_required_text(teammate_id.as_str()) else {
                    self.send_invalid_request_error(
                        request_id,
                        "teammate_id must not be empty".to_string(),
                    )
                    .await;
                    return;
                };
                teammate_id
            }
            None => new_teammate_id(),
        };
        let Some(lead_spawn_context) = self
            .resolve_team_lead_spawn_context(&state_db_ctx, team_state.lead_thread_id.as_str())
            .await
        else {
            self.send_internal_error(
                request_id,
                format!(
                    "failed to resolve lead spawn context for thread {}",
                    team_state.lead_thread_id
                ),
            )
            .await;
            return;
        };
        let teammate_agent_path = match lead_spawn_context
            .parent_agent_path
            .join(teammate_id.as_str())
        {
            Ok(agent_path) => agent_path,
            Err(err) => {
                self.send_invalid_request_error(
                    request_id,
                    format!("invalid teammate_id for agent path: {err}"),
                )
                .await;
                return;
            }
        };
        match state_db_ctx
            .get_team_teammate(team_state.id.as_str(), teammate_id.as_str())
            .await
        {
            Ok(Some(_)) => {
                self.send_invalid_request_error(
                    request_id,
                    format!(
                        "teammate already exists in team {}: {teammate_id}",
                        team_state.id
                    ),
                )
                .await;
                return;
            }
            Ok(None) => {}
            Err(err) => {
                self.send_internal_error(
                    request_id,
                    format!(
                        "failed to check existing teammate {teammate_id} in team {}: {err}",
                        team_state.id
                    ),
                )
                .await;
                return;
            }
        }

        if let Err(err) = state_db_ctx
            .create_team_teammate(&praxis_state::TeamTeammateCreateParams {
                team_id: team_state.id.clone(),
                teammate_id: teammate_id.clone(),
                name: name.clone(),
                role: role.clone(),
                status: praxis_state::TeamTeammateStatus::Pending,
                thread_id: None,
                last_error: None,
            })
            .await
        {
            self.send_internal_error(
                request_id,
                format!(
                    "failed to create teammate record for team {}: {err}",
                    team_state.id
                ),
            )
            .await;
            return;
        }

        let lead_defaults = self
            .resolve_team_lead_defaults(team_state.lead_thread_id.as_str())
            .await;
        let developer_instructions = params.developer_instructions.or_else(|| {
            Some(default_teammate_developer_instructions(
                team_state.name.as_str(),
                team_state.objective.as_deref(),
                name.as_str(),
                role.as_deref(),
            ))
        });
        let mut typesafe_overrides = self.build_thread_config_overrides(
            params.model.or(lead_defaults.model),
            params.model_provider.or(lead_defaults.model_provider),
            None,
            params.cwd.or(lead_defaults.cwd),
            params.approval_policy,
            params.approvals_reviewer,
            params.sandbox,
            params.base_instructions,
            developer_instructions,
            params.personality,
        );
        typesafe_overrides.ephemeral = params.ephemeral;

        let cloud_requirements = self.current_cloud_requirements();
        let cli_overrides = self.current_cli_overrides();
        let runtime_feature_enablement = self.current_runtime_feature_enablement();
        let config = match derive_config_from_params(
            &cli_overrides,
            None,
            typesafe_overrides,
            &cloud_requirements,
            &self.config.praxis_home,
            &runtime_feature_enablement,
        )
        .await
        {
            Ok(config) => config,
            Err(err) => {
                let error = config_load_error(&err);
                let _ = state_db_ctx
                    .set_team_teammate_status(
                        team_state.id.as_str(),
                        teammate_id.as_str(),
                        praxis_state::TeamTeammateStatus::Failed,
                        None,
                        Some(error.message.as_str()),
                    )
                    .await;
                self.outgoing.send_error(request_id, error).await;
                return;
            }
        };

        let request_trace = self.request_trace_context(&request_id).await;
        let fallback_model_provider = config.model_provider_id.clone();
        let service_name = params.service_name;
        let teammate_session_source = SessionSource::SubAgent(SubAgentSource::ThreadSpawn {
            parent_thread_id: lead_spawn_context.parent_thread_id,
            depth: lead_spawn_context.parent_depth.saturating_add(1),
            agent_path: Some(teammate_agent_path),
            agent_nickname: Some(name.clone()),
            agent_role: role.clone(),
        });
        let new_thread = match self
            .thread_manager
            .start_thread_with_tools_and_source_and_service_name(
                config,
                teammate_session_source,
                Vec::<CoreDynamicToolSpec>::new(),
                /*persist_extended_history*/ false,
                service_name,
                request_trace,
            )
            .await
        {
            Ok(thread) => thread,
            Err(err) => {
                let message = format!("failed to create teammate thread: {err}");
                let _ = state_db_ctx
                    .set_team_teammate_status(
                        team_state.id.as_str(),
                        teammate_id.as_str(),
                        praxis_state::TeamTeammateStatus::Failed,
                        None,
                        Some(message.as_str()),
                    )
                    .await;
                self.send_internal_error(request_id, message).await;
                return;
            }
        };

        let NewThread {
            thread_id,
            thread,
            session_configured,
            ..
        } = new_thread;
        let config_snapshot = thread.config_snapshot().await;
        let mut api_thread = build_thread_from_snapshot(
            thread_id,
            &config_snapshot,
            session_configured.rollout_path.clone(),
        );
        Self::log_listener_attach_result(
            self.ensure_conversation_listener(
                thread_id,
                request_id.connection_id,
                /*raw_events_enabled*/ false,
            )
            .await,
            thread_id,
            request_id.connection_id,
            "thread",
        );
        self.thread_watch_manager
            .upsert_thread_silently(api_thread.clone())
            .await;
        api_thread.status = resolve_thread_status(
            self.thread_watch_manager
                .loaded_status_for_thread(&api_thread.id)
                .await,
            /*has_in_progress_turn*/ false,
        );

        let thread_id_text = thread_id.to_string();
        if let Err(err) = state_db_ctx
            .set_team_teammate_status(
                team_state.id.as_str(),
                teammate_id.as_str(),
                praxis_state::TeamTeammateStatus::Active,
                Some(thread_id_text.as_str()),
                None,
            )
            .await
        {
            self.send_internal_error(
                request_id,
                format!(
                    "failed to persist teammate thread {} for team {}: {err}",
                    teammate_id, team_state.id
                ),
            )
            .await;
            let _ = self.thread_manager.remove_thread(&thread_id).await;
            let _ = Self::wait_for_thread_shutdown(&thread).await;
            self.finalize_thread_teardown(thread_id).await;
            return;
        }

        let teammate_state = match state_db_ctx
            .get_team_teammate(team_state.id.as_str(), teammate_id.as_str())
            .await
        {
            Ok(Some(teammate)) => teammate,
            Ok(None) => {
                self.send_internal_error(
                    request_id,
                    format!(
                        "teammate {} disappeared after creation for team {}",
                        teammate_id, team_state.id
                    ),
                )
                .await;
                return;
            }
            Err(err) => {
                self.send_internal_error(
                    request_id,
                    format!(
                        "failed to reload teammate {} for team {}: {err}",
                        teammate_id, team_state.id
                    ),
                )
                .await;
                return;
            }
        };
        let team_state = match state_db_ctx.get_team(team_state.id.as_str()).await {
            Ok(Some(team)) => team,
            Ok(None) => {
                self.send_internal_error(
                    request_id,
                    format!("team {} disappeared after teammate creation", team_state.id),
                )
                .await;
                return;
            }
            Err(err) => {
                self.send_internal_error(
                    request_id,
                    format!("failed to reload team {}: {err}", team_state.id),
                )
                .await;
                return;
            }
        };

        if api_thread.model_provider.is_empty() {
            api_thread.model_provider = fallback_model_provider;
        }
        let api_team = api_team_from_state(&team_state);
        let api_teammate = api_team_teammate_from_state(&teammate_state);
        self.outgoing
            .send_response(
                request_id,
                TeamTeammateCreateResponse {
                    team: api_team.clone(),
                    teammate: api_teammate.clone(),
                    thread: api_thread.clone(),
                },
            )
            .await;
        self.outgoing
            .send_server_notification(ServerNotification::ThreadStarted(
                ThreadStartedNotification {
                    thread: api_thread.clone(),
                },
            ))
            .await;
        self.outgoing
            .send_server_notification(ServerNotification::TeamTeammateUpdated(
                TeamTeammateUpdatedNotification {
                    team_id: api_team.id,
                    teammate: api_teammate,
                    thread: Some(api_thread),
                },
            ))
            .await;
    }

    pub(super) async fn team_teammate_message(
        &self,
        request_id: ConnectionRequestId,
        params: TeamTeammateMessageParams,
    ) {
        let Some(state_db_ctx) = self.require_team_state_db(&request_id).await else {
            return;
        };
        let Some(team_state) = self
            .load_team_or_send_invalid(&request_id, &state_db_ctx, params.team_id.as_str())
            .await
        else {
            return;
        };
        let Some(body) = normalize_required_text(params.body.as_str()) else {
            self.send_invalid_request_error(
                request_id,
                "team mailbox message body must not be empty".to_string(),
            )
            .await;
            return;
        };
        if participants_equal(&params.sender, &params.recipient) {
            self.send_invalid_request_error(
                request_id,
                "sender and recipient must be different".to_string(),
            )
            .await;
            return;
        }
        if !self
            .validate_team_participant(
                &request_id,
                &state_db_ctx,
                team_state.id.as_str(),
                &params.sender,
                "sender",
            )
            .await
        {
            return;
        }
        if !self
            .validate_team_participant(
                &request_id,
                &state_db_ctx,
                team_state.id.as_str(),
                &params.recipient,
                "recipient",
            )
            .await
        {
            return;
        }

        let message_id = params.message_id.unwrap_or_else(new_message_id);
        match state_db_ctx
            .get_team_mailbox_message(message_id.as_str())
            .await
        {
            Ok(Some(_)) => {
                self.send_invalid_request_error(
                    request_id,
                    format!("team mailbox message already exists: {message_id}"),
                )
                .await;
                return;
            }
            Ok(None) => {}
            Err(err) => {
                self.send_internal_error(
                    request_id,
                    format!("failed to check existing team mailbox message {message_id}: {err}"),
                )
                .await;
                return;
            }
        }

        let (sender_kind, sender_teammate_id) = state_participant_from_api(&params.sender);
        let (recipient_kind, recipient_teammate_id) = state_participant_from_api(&params.recipient);
        let message = match state_db_ctx
            .create_team_mailbox_message(&praxis_state::TeamMailboxMessageCreateParams {
                id: message_id,
                team_id: team_state.id.clone(),
                sender_kind,
                sender_teammate_id,
                recipient_kind,
                recipient_teammate_id,
                body,
            })
            .await
        {
            Ok(message) => message,
            Err(err) => {
                self.send_internal_error(
                    request_id,
                    format!("failed to create team mailbox message: {err}"),
                )
                .await;
                return;
            }
        };
        self.deliver_team_mailbox_message_live(
            &request_id,
            &state_db_ctx,
            &team_state,
            &params.sender,
            &params.recipient,
            message.body.as_str(),
        )
        .await;

        let message = api_team_mailbox_message_from_state(&message);
        self.outgoing
            .send_response(
                request_id,
                TeamTeammateMessageResponse {
                    message: message.clone(),
                },
            )
            .await;
        self.outgoing
            .send_server_notification(ServerNotification::TeamMailboxUpdated(
                TeamMailboxUpdatedNotification {
                    team_id: team_state.id,
                    message,
                },
            ))
            .await;
    }

    pub(super) async fn team_task_create(
        &self,
        request_id: ConnectionRequestId,
        params: TeamTaskCreateParams,
    ) {
        let Some(state_db_ctx) = self.require_team_state_db(&request_id).await else {
            return;
        };
        let Some(team_state) = self
            .load_team_or_send_invalid(&request_id, &state_db_ctx, params.team_id.as_str())
            .await
        else {
            return;
        };
        let Some(title) = normalize_required_text(params.title.as_str()) else {
            self.send_invalid_request_error(
                request_id,
                "team task title must not be empty".to_string(),
            )
            .await;
            return;
        };
        let description = normalize_optional_text(params.description);
        let assignee_teammate_id = normalize_optional_text(params.assignee_teammate_id);
        if let Some(assignee_teammate_id) = assignee_teammate_id.as_ref()
            && !self
                .team_teammate_exists(
                    &request_id,
                    &state_db_ctx,
                    team_state.id.as_str(),
                    assignee_teammate_id,
                )
                .await
        {
            return;
        }

        let task_id = params.task_id.unwrap_or_else(new_task_id);
        match state_db_ctx
            .get_team_task(team_state.id.as_str(), task_id.as_str())
            .await
        {
            Ok(Some(_)) => {
                self.send_invalid_request_error(
                    request_id,
                    format!(
                        "team task already exists in team {}: {task_id}",
                        team_state.id
                    ),
                )
                .await;
                return;
            }
            Ok(None) => {}
            Err(err) => {
                self.send_internal_error(
                    request_id,
                    format!(
                        "failed to check existing task {task_id} in team {}: {err}",
                        team_state.id
                    ),
                )
                .await;
                return;
            }
        }

        let task = match state_db_ctx
            .create_team_task(&praxis_state::TeamTaskCreateParams {
                team_id: team_state.id.clone(),
                task_id,
                title,
                description,
                status: praxis_state::TeamTaskStatus::Pending,
                assignee_teammate_id,
            })
            .await
        {
            Ok(task) => task,
            Err(err) => {
                self.send_internal_error(request_id, format!("failed to create team task: {err}"))
                    .await;
                return;
            }
        };

        let task = api_team_task_from_state(&task);
        self.outgoing
            .send_response(request_id, TeamTaskCreateResponse { task: task.clone() })
            .await;
        self.outgoing
            .send_server_notification(ServerNotification::TeamTaskUpdated(
                TeamTaskUpdatedNotification {
                    team_id: team_state.id,
                    task,
                },
            ))
            .await;
    }

    pub(super) async fn team_task_list(
        &self,
        request_id: ConnectionRequestId,
        params: TeamTaskListParams,
    ) {
        let Some(state_db_ctx) = self.require_team_state_db(&request_id).await else {
            return;
        };
        let Some(team_state) = self
            .load_team_or_send_invalid(&request_id, &state_db_ctx, params.team_id.as_str())
            .await
        else {
            return;
        };
        let tasks = match state_db_ctx.list_team_tasks(team_state.id.as_str()).await {
            Ok(tasks) => tasks
                .iter()
                .map(api_team_task_from_state)
                .collect::<Vec<_>>(),
            Err(err) => {
                self.send_internal_error(
                    request_id,
                    format!("failed to list tasks for team {}: {err}", team_state.id),
                )
                .await;
                return;
            }
        };
        self.outgoing
            .send_response(request_id, TeamTaskListResponse { data: tasks })
            .await;
    }

    pub(super) async fn team_task_update(
        &self,
        request_id: ConnectionRequestId,
        params: TeamTaskUpdateParams,
    ) {
        let Some(state_db_ctx) = self.require_team_state_db(&request_id).await else {
            return;
        };
        let Some(team_state) = self
            .load_team_or_send_invalid(&request_id, &state_db_ctx, params.team_id.as_str())
            .await
        else {
            return;
        };

        let title = params
            .title
            .and_then(|value| normalize_optional_text(Some(value)));
        let description = params
            .description
            .and_then(|value| normalize_optional_text(Some(value)));
        let assignee_teammate_id = params
            .assignee_teammate_id
            .and_then(|value| normalize_optional_text(Some(value)));
        if let Some(assignee_teammate_id) = assignee_teammate_id.as_ref()
            && !params.clear_assignee
            && !self
                .team_teammate_exists(
                    &request_id,
                    &state_db_ctx,
                    team_state.id.as_str(),
                    assignee_teammate_id,
                )
                .await
        {
            return;
        }

        let task = match state_db_ctx
            .update_team_task(&praxis_state::TeamTaskUpdateParams {
                team_id: team_state.id.clone(),
                task_id: params.task_id.clone(),
                title,
                description,
                status: params.status.map(state_team_task_status_from_api),
                assignee_teammate_id,
                clear_assignee: params.clear_assignee,
            })
            .await
        {
            Ok(Some(task)) => task,
            Ok(None) => {
                self.send_invalid_request_error(
                    request_id,
                    format!(
                        "team task not found in team {}: {}",
                        team_state.id, params.task_id
                    ),
                )
                .await;
                return;
            }
            Err(err) => {
                self.send_internal_error(request_id, format!("failed to update team task: {err}"))
                    .await;
                return;
            }
        };

        let task = api_team_task_from_state(&task);
        self.outgoing
            .send_response(request_id, TeamTaskUpdateResponse { task: task.clone() })
            .await;
        self.outgoing
            .send_server_notification(ServerNotification::TeamTaskUpdated(
                TeamTaskUpdatedNotification {
                    team_id: team_state.id,
                    task,
                },
            ))
            .await;
    }

    async fn require_team_state_db(
        &self,
        request_id: &ConnectionRequestId,
    ) -> Option<StateDbHandle> {
        let Some(state_db_ctx) = get_state_db(&self.config).await else {
            self.send_internal_error(
                request_id.clone(),
                "state database is unavailable".to_string(),
            )
            .await;
            return None;
        };
        Some(state_db_ctx)
    }

    async fn require_known_thread_reference(
        &self,
        request_id: &ConnectionRequestId,
        thread_id: &str,
    ) -> Option<ThreadId> {
        let thread_id = match ThreadId::from_string(thread_id) {
            Ok(thread_id) => thread_id,
            Err(err) => {
                self.send_invalid_request_error(
                    request_id.clone(),
                    format!("invalid thread id: {err}"),
                )
                .await;
                return None;
            }
        };
        if self.thread_manager.get_thread(thread_id).await.is_ok() {
            return Some(thread_id);
        }
        let directory = praxis_rollout::ThreadDirectory::open(&self.config).await;
        match directory.thread_exists(thread_id, None).await {
            Ok(true) => Some(thread_id),
            Ok(false) => {
                self.send_invalid_request_error(
                    request_id.clone(),
                    format!("thread not found: {thread_id}"),
                )
                .await;
                None
            }
            Err(err) => {
                self.send_invalid_request_error(
                    request_id.clone(),
                    format!("failed to locate thread id {thread_id}: {err}"),
                )
                .await;
                None
            }
        }
    }

    async fn load_team_or_send_invalid(
        &self,
        request_id: &ConnectionRequestId,
        state_db_ctx: &StateDbHandle,
        team_id: &str,
    ) -> Option<praxis_state::Team> {
        match state_db_ctx.get_team(team_id).await {
            Ok(Some(team)) => Some(team),
            Ok(None) => {
                self.send_invalid_request_error(
                    request_id.clone(),
                    format!("team not found: {team_id}"),
                )
                .await;
                None
            }
            Err(err) => {
                self.send_internal_error(
                    request_id.clone(),
                    format!("failed to load team {team_id}: {err}"),
                )
                .await;
                None
            }
        }
    }

    async fn team_teammate_exists(
        &self,
        request_id: &ConnectionRequestId,
        state_db_ctx: &StateDbHandle,
        team_id: &str,
        teammate_id: &str,
    ) -> bool {
        match state_db_ctx.get_team_teammate(team_id, teammate_id).await {
            Ok(Some(_)) => true,
            Ok(None) => {
                self.send_invalid_request_error(
                    request_id.clone(),
                    format!("teammate not found in team {team_id}: {teammate_id}"),
                )
                .await;
                false
            }
            Err(err) => {
                self.send_internal_error(
                    request_id.clone(),
                    format!("failed to load teammate {teammate_id} in team {team_id}: {err}"),
                )
                .await;
                false
            }
        }
    }

    async fn validate_team_participant(
        &self,
        request_id: &ConnectionRequestId,
        state_db_ctx: &StateDbHandle,
        team_id: &str,
        participant: &TeamMailboxParticipant,
        label: &str,
    ) -> bool {
        match participant {
            TeamMailboxParticipant::Lead => true,
            TeamMailboxParticipant::Teammate { teammate_id } => {
                let Some(teammate_id) = normalize_required_text(teammate_id.as_str()) else {
                    self.send_invalid_request_error(
                        request_id.clone(),
                        format!("{label} teammate id must not be empty"),
                    )
                    .await;
                    return false;
                };
                self.team_teammate_exists(request_id, state_db_ctx, team_id, teammate_id.as_str())
                    .await
            }
        }
    }

    async fn resolve_team_lead_defaults(&self, lead_thread_id: &str) -> TeamLeadDefaults {
        let Ok(thread_id) = ThreadId::from_string(lead_thread_id) else {
            return TeamLeadDefaults::default();
        };
        if let Ok(thread) = self.thread_manager.get_thread(thread_id).await {
            let snapshot = thread.config_snapshot().await;
            return TeamLeadDefaults {
                model: Some(snapshot.model),
                model_provider: Some(snapshot.model_provider_id),
                cwd: Some(snapshot.cwd.to_string_lossy().into_owned()),
            };
        }
        if let Some(state_db_ctx) = get_state_db(&self.config).await
            && let Ok(Some(metadata)) = state_db_ctx.get_thread(thread_id).await
        {
            return TeamLeadDefaults {
                model: metadata.model,
                model_provider: Some(metadata.model_provider),
                cwd: Some(metadata.cwd.to_string_lossy().into_owned()),
            };
        }
        TeamLeadDefaults::default()
    }

    async fn resolve_team_lead_spawn_context(
        &self,
        state_db_ctx: &StateDbHandle,
        lead_thread_id: &str,
    ) -> Option<TeamLeadSpawnContext> {
        let parent_thread_id = ThreadId::from_string(lead_thread_id).ok()?;
        if let Ok(thread) = self.thread_manager.get_thread(parent_thread_id).await {
            let snapshot = thread.config_snapshot().await;
            return Some(TeamLeadSpawnContext {
                parent_thread_id,
                parent_depth: team_session_source_depth(&snapshot.session_source),
                parent_agent_path: snapshot
                    .session_source
                    .get_agent_path()
                    .unwrap_or_else(AgentPath::root),
            });
        }

        if let Ok(Some(metadata)) = state_db_ctx.get_thread(parent_thread_id).await {
            let source = parse_session_source_str(metadata.source.as_str());
            let parent_agent_path = metadata
                .agent_path
                .as_deref()
                .and_then(|agent_path| AgentPath::try_from(agent_path).ok())
                .or_else(|| source.as_ref().and_then(SessionSource::get_agent_path))
                .unwrap_or_else(AgentPath::root);
            return Some(TeamLeadSpawnContext {
                parent_thread_id,
                parent_depth: source.as_ref().map(team_session_source_depth).unwrap_or(0),
                parent_agent_path,
            });
        }

        Some(TeamLeadSpawnContext {
            parent_thread_id,
            parent_depth: 0,
            parent_agent_path: AgentPath::root(),
        })
    }

    async fn deliver_team_mailbox_message_live(
        &self,
        request_id: &ConnectionRequestId,
        state_db_ctx: &StateDbHandle,
        team: &praxis_state::Team,
        sender: &TeamMailboxParticipant,
        recipient: &TeamMailboxParticipant,
        body: &str,
    ) {
        let Some(target_thread_id) = self
            .resolve_team_mailbox_target_thread_id(state_db_ctx, team, recipient)
            .await
        else {
            return;
        };
        let Ok(thread) = self.thread_manager.get_thread(target_thread_id).await else {
            return;
        };
        let communication = InterAgentCommunication::new(
            self.resolve_team_mailbox_participant_agent_path(state_db_ctx, team, sender)
                .await,
            self.resolve_team_mailbox_participant_agent_path(state_db_ctx, team, recipient)
                .await,
            Vec::new(),
            format_team_live_mailbox_message(sender, body),
            /*trigger_turn*/ true,
        );
        let _ = thread
            .submit_with_trace(
                Op::InterAgentCommunication { communication },
                self.request_trace_context(request_id).await,
            )
            .await;
    }

    async fn resolve_team_mailbox_target_thread_id(
        &self,
        state_db_ctx: &StateDbHandle,
        team: &praxis_state::Team,
        participant: &TeamMailboxParticipant,
    ) -> Option<ThreadId> {
        match participant {
            TeamMailboxParticipant::Lead => {
                ThreadId::from_string(team.lead_thread_id.as_str()).ok()
            }
            TeamMailboxParticipant::Teammate { teammate_id } => {
                let teammate_id = normalize_required_text(teammate_id.as_str())?;
                let Ok(Some(teammate)) = state_db_ctx
                    .get_team_teammate(team.id.as_str(), teammate_id.as_str())
                    .await
                else {
                    return None;
                };
                teammate
                    .thread_id
                    .as_deref()
                    .and_then(|thread_id| ThreadId::from_string(thread_id).ok())
            }
        }
    }

    async fn resolve_team_mailbox_participant_agent_path(
        &self,
        state_db_ctx: &StateDbHandle,
        team: &praxis_state::Team,
        participant: &TeamMailboxParticipant,
    ) -> AgentPath {
        match participant {
            TeamMailboxParticipant::Lead => self
                .resolve_team_thread_agent_path(state_db_ctx, team.lead_thread_id.as_str())
                .await
                .unwrap_or_else(AgentPath::root),
            TeamMailboxParticipant::Teammate { teammate_id } => {
                let teammate_id = normalize_required_text(teammate_id.as_str())
                    .unwrap_or_else(|| teammate_id.clone());
                if let Ok(Some(teammate)) = state_db_ctx
                    .get_team_teammate(team.id.as_str(), teammate_id.as_str())
                    .await
                {
                    if let Some(thread_id) = teammate.thread_id.as_deref()
                        && let Some(agent_path) = self
                            .resolve_team_thread_agent_path(state_db_ctx, thread_id)
                            .await
                    {
                        return agent_path;
                    }
                }
                self.resolve_team_lead_spawn_context(state_db_ctx, team.lead_thread_id.as_str())
                    .await
                    .and_then(|ctx| ctx.parent_agent_path.join(teammate_id.as_str()).ok())
                    .unwrap_or_else(AgentPath::root)
            }
        }
    }

    async fn resolve_team_thread_agent_path(
        &self,
        state_db_ctx: &StateDbHandle,
        thread_id: &str,
    ) -> Option<AgentPath> {
        let thread_id = ThreadId::from_string(thread_id).ok()?;
        if let Ok(thread) = self.thread_manager.get_thread(thread_id).await {
            let snapshot = thread.config_snapshot().await;
            return Some(
                snapshot
                    .session_source
                    .get_agent_path()
                    .unwrap_or_else(AgentPath::root),
            );
        }

        let metadata = state_db_ctx.get_thread(thread_id).await.ok().flatten()?;
        metadata
            .agent_path
            .as_deref()
            .and_then(|agent_path| AgentPath::try_from(agent_path).ok())
            .or_else(|| {
                parse_session_source_str(metadata.source.as_str())
                    .and_then(|source| source.get_agent_path())
            })
            .or_else(|| Some(AgentPath::root()))
    }

    pub(super) async fn sync_closed_team_teammate_for_thread(
        config: &Arc<Config>,
        outgoing: &Arc<OutgoingMessageSender>,
        thread_id: ThreadId,
    ) {
        let Some(state_db_ctx) = get_state_db(config).await else {
            return;
        };
        let thread_id_text = thread_id.to_string();
        let Ok(Some(teammate)) = state_db_ctx
            .get_team_teammate_by_thread_id(thread_id_text.as_str())
            .await
        else {
            return;
        };
        if let Err(err) = state_db_ctx
            .set_team_teammate_status(
                teammate.team_id.as_str(),
                teammate.teammate_id.as_str(),
                praxis_state::TeamTeammateStatus::Closed,
                Some(thread_id_text.as_str()),
                None,
            )
            .await
        {
            warn!(
                "failed to mark teammate {} in team {} closed for thread {}: {err}",
                teammate.teammate_id, teammate.team_id, thread_id
            );
            return;
        }
        let Ok(Some(updated_teammate)) = state_db_ctx
            .get_team_teammate(teammate.team_id.as_str(), teammate.teammate_id.as_str())
            .await
        else {
            return;
        };
        outgoing
            .send_server_notification(ServerNotification::TeamTeammateUpdated(
                TeamTeammateUpdatedNotification {
                    team_id: updated_teammate.team_id.clone(),
                    teammate: api_team_teammate_from_state(&updated_teammate),
                    thread: None,
                },
            ))
            .await;
    }
}

fn api_team_from_state(team: &praxis_state::Team) -> Team {
    Team {
        id: team.id.clone(),
        lead_thread_id: team.lead_thread_id.clone(),
        name: team.name.clone(),
        objective: team.objective.clone(),
        execution_mode: match team.execution_mode {
            praxis_state::TeamExecutionMode::ProcessFirst => TeamExecutionMode::ProcessFirst,
        },
        resume_mode: match team.resume_mode {
            praxis_state::TeamResumeMode::Strong => TeamResumeMode::StrongResume,
        },
        created_at: team.created_at.timestamp(),
        updated_at: team.updated_at.timestamp(),
    }
}

fn api_team_teammate_from_state(teammate: &praxis_state::TeamTeammate) -> TeamTeammate {
    TeamTeammate {
        team_id: teammate.team_id.clone(),
        teammate_id: teammate.teammate_id.clone(),
        name: teammate.name.clone(),
        role: teammate.role.clone(),
        status: match teammate.status {
            praxis_state::TeamTeammateStatus::Pending => TeamTeammateStatus::Pending,
            praxis_state::TeamTeammateStatus::Active => TeamTeammateStatus::Active,
            praxis_state::TeamTeammateStatus::Failed => TeamTeammateStatus::Failed,
            praxis_state::TeamTeammateStatus::Closed => TeamTeammateStatus::Closed,
        },
        thread_id: teammate.thread_id.clone(),
        last_error: teammate.last_error.clone(),
        created_at: teammate.created_at.timestamp(),
        updated_at: teammate.updated_at.timestamp(),
    }
}

fn api_team_task_from_state(task: &praxis_state::TeamTask) -> TeamTask {
    TeamTask {
        team_id: task.team_id.clone(),
        task_id: task.task_id.clone(),
        title: task.title.clone(),
        description: task.description.clone(),
        status: match task.status {
            praxis_state::TeamTaskStatus::Pending => TeamTaskStatus::Pending,
            praxis_state::TeamTaskStatus::InProgress => TeamTaskStatus::InProgress,
            praxis_state::TeamTaskStatus::Blocked => TeamTaskStatus::Blocked,
            praxis_state::TeamTaskStatus::Completed => TeamTaskStatus::Completed,
        },
        assignee_teammate_id: task.assignee_teammate_id.clone(),
        created_at: task.created_at.timestamp(),
        updated_at: task.updated_at.timestamp(),
        completed_at: task.completed_at.map(|value| value.timestamp()),
    }
}

fn api_team_mailbox_message_from_state(
    message: &praxis_state::TeamMailboxMessage,
) -> TeamMailboxMessage {
    TeamMailboxMessage {
        id: message.id.clone(),
        team_id: message.team_id.clone(),
        sender: api_participant_from_state(
            message.sender_kind,
            message.sender_teammate_id.as_ref(),
        ),
        recipient: api_participant_from_state(
            message.recipient_kind,
            message.recipient_teammate_id.as_ref(),
        ),
        body: message.body.clone(),
        created_at: message.created_at.timestamp(),
    }
}

fn api_participant_from_state(
    kind: praxis_state::TeamMailboxParticipantKind,
    teammate_id: Option<&String>,
) -> TeamMailboxParticipant {
    match kind {
        praxis_state::TeamMailboxParticipantKind::Lead => TeamMailboxParticipant::Lead,
        praxis_state::TeamMailboxParticipantKind::Teammate => TeamMailboxParticipant::Teammate {
            teammate_id: teammate_id.cloned().unwrap_or_default(),
        },
    }
}

fn state_participant_from_api(
    participant: &TeamMailboxParticipant,
) -> (praxis_state::TeamMailboxParticipantKind, Option<String>) {
    match participant {
        TeamMailboxParticipant::Lead => (praxis_state::TeamMailboxParticipantKind::Lead, None),
        TeamMailboxParticipant::Teammate { teammate_id } => (
            praxis_state::TeamMailboxParticipantKind::Teammate,
            Some(teammate_id.trim().to_string()),
        ),
    }
}

fn state_team_task_status_from_api(status: TeamTaskStatus) -> praxis_state::TeamTaskStatus {
    match status {
        TeamTaskStatus::Pending => praxis_state::TeamTaskStatus::Pending,
        TeamTaskStatus::InProgress => praxis_state::TeamTaskStatus::InProgress,
        TeamTaskStatus::Blocked => praxis_state::TeamTaskStatus::Blocked,
        TeamTaskStatus::Completed => praxis_state::TeamTaskStatus::Completed,
    }
}

fn normalize_required_text(value: &str) -> Option<String> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

fn normalize_optional_text(value: Option<String>) -> Option<String> {
    value.and_then(|value| normalize_required_text(value.as_str()))
}

fn participants_equal(left: &TeamMailboxParticipant, right: &TeamMailboxParticipant) -> bool {
    match (left, right) {
        (TeamMailboxParticipant::Lead, TeamMailboxParticipant::Lead) => true,
        (
            TeamMailboxParticipant::Teammate {
                teammate_id: left_id,
            },
            TeamMailboxParticipant::Teammate {
                teammate_id: right_id,
            },
        ) => left_id.trim() == right_id.trim(),
        _ => false,
    }
}

fn format_team_live_mailbox_message(sender: &TeamMailboxParticipant, body: &str) -> String {
    let sender_label = match sender {
        TeamMailboxParticipant::Lead => "team lead".to_string(),
        TeamMailboxParticipant::Teammate { teammate_id } => format!("teammate {teammate_id}"),
    };
    format!(
        "Team mailbox message from {sender_label}. Use team_read to inspect the latest team state if needed.\n\n{body}"
    )
}

fn default_teammate_developer_instructions(
    team_name: &str,
    objective: Option<&str>,
    teammate_name: &str,
    role: Option<&str>,
) -> String {
    let role_clause = role
        .map(|role| format!(" Role: {role}."))
        .unwrap_or_default();
    let objective_clause = objective
        .map(|objective| format!(" Team objective: {objective}."))
        .unwrap_or_default();
    format!(
        "You are teammate {teammate_name} in team {team_name}.{role_clause}{objective_clause} Use team_read to inspect the current team context, team_task_list/team_task_create/team_task_update to manage work, and team_send_message to coordinate with the lead or other teammates. The current thread's team context is inferred automatically. Keep your updates concise."
    )
}

fn team_session_source_depth(source: &SessionSource) -> i32 {
    match source {
        SessionSource::SubAgent(SubAgentSource::ThreadSpawn { depth, .. }) => *depth,
        _ => 0,
    }
}

fn parse_session_source_str(source: &str) -> Option<SessionSource> {
    serde_json::from_str(source)
        .or_else(|_| serde_json::from_value(serde_json::Value::String(source.to_string())))
        .ok()
}

fn new_team_id() -> String {
    format!("team_{}", Uuid::now_v7().simple())
}

fn new_teammate_id() -> String {
    format!("mate_{}", Uuid::now_v7().simple())
}

fn new_task_id() -> String {
    format!("task_{}", Uuid::now_v7().simple())
}

fn new_message_id() -> String {
    format!("msg_{}", Uuid::now_v7().simple())
}
