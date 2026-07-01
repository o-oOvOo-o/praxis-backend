/*
This module holds the temporary adapter layer between the TUI and the app
server during the hybrid migration period.

For now, the TUI still owns its existing direct-core behavior, but startup
allocates a local in-process app gateway and drains its event stream. Keeping
the app-gateway-specific wiring here keeps that transitional logic out of the
main `app.rs` orchestration path.

As more TUI flows move onto the app-gateway surface directly, this adapter
should shrink and eventually disappear.
*/

use super::App;
use crate::app_gateway_session::AppGatewaySession;
use crate::app_gateway_session::app_gateway_rate_limit_snapshot_to_core;
use crate::app_gateway_session::status_account_display_from_auth_mode;
use praxis_app_gateway_client::AppGatewayEvent;
use praxis_app_gateway_protocol::AuthMode;
use praxis_app_gateway_protocol::JSONRPCErrorError;
use praxis_app_gateway_protocol::ServerNotification;
use praxis_app_gateway_protocol::ServerRequest;
use praxis_protocol::ThreadId;

impl App {
    fn refresh_mcp_startup_expected_servers_from_config(&mut self) {
        let enabled_config_mcp_servers: Vec<String> = self
            .chat_widget
            .config_ref()
            .mcp_servers
            .get()
            .iter()
            .filter_map(|(name, server)| server.enabled.then_some(name.clone()))
            .collect();
        self.chat_widget
            .set_mcp_startup_expected_servers(enabled_config_mcp_servers);
    }

    pub(super) async fn handle_app_gateway_event(
        &mut self,
        app_gateway_client: &mut AppGatewaySession,
        event: AppGatewayEvent,
    ) {
        match event {
            AppGatewayEvent::Lagged { skipped } => {
                tracing::warn!(
                    skipped,
                    "app-gateway event consumer lagged; dropping ignored events"
                );
                self.refresh_mcp_startup_expected_servers_from_config();
                self.chat_widget.finish_mcp_startup_after_lag();
                self.refresh_workspace_threads(app_gateway_client, /*force*/ true);
            }
            AppGatewayEvent::ServerNotification(notification) => {
                self.handle_server_notification_event(app_gateway_client, notification)
                    .await;
            }
            AppGatewayEvent::ServerRequest(request) => {
                self.handle_server_request_event(app_gateway_client, request)
                    .await;
            }
            AppGatewayEvent::Disconnected { message } => {
                tracing::warn!("app-gateway event stream disconnected: {message}");
                self.mark_app_gateway_disconnected(message);
            }
        }
    }

    async fn handle_server_notification_event(
        &mut self,
        app_gateway_client: &mut AppGatewaySession,
        notification: ServerNotification,
    ) {
        match &notification {
            ServerNotification::ServerRequestResolved(notification) => {
                self.pending_app_gateway_requests
                    .resolve_notification(&notification.request_id);
            }
            ServerNotification::McpServerStatusUpdated(_) => {
                self.refresh_mcp_startup_expected_servers_from_config();
            }
            ServerNotification::AccountRateLimitsUpdated(notification) => {
                self.chat_widget.on_rate_limit_snapshot(Some(
                    app_gateway_rate_limit_snapshot_to_core(notification.rate_limits.clone()),
                ));
                return;
            }
            ServerNotification::AccountUpdated(notification) => {
                self.chat_widget.update_account_state(
                    status_account_display_from_auth_mode(
                        notification.auth_mode,
                        notification.plan_type,
                    ),
                    notification.plan_type,
                    matches!(
                        notification.auth_mode,
                        Some(AuthMode::Chatgpt) | Some(AuthMode::ChatgptAuthTokens)
                    ),
                );
                return;
            }
            _ => {}
        }

        if self.apply_workspace_server_notification(&notification) {
            self.refresh_workspace_threads(app_gateway_client, /*force*/ true);
        }

        let thread_to_observe = self.workspace_observable_notification_thread_id(&notification);
        if let Some(thread_id) = thread_to_observe {
            self.observe_workspace_thread_if_needed(app_gateway_client, thread_id)
                .await;
        }

        match server_notification_thread_target(&notification) {
            ServerNotificationThreadTarget::Thread(thread_id) => {
                let result = if self.primary_thread_id == Some(thread_id)
                    || self.primary_thread_id.is_none()
                {
                    self.enqueue_primary_thread_notification(notification).await
                } else {
                    self.enqueue_thread_notification(thread_id, notification)
                        .await
                };

                if let Err(err) = result {
                    tracing::warn!("failed to enqueue app-gateway notification: {err}");
                }
                return;
            }
            ServerNotificationThreadTarget::InvalidThreadId(thread_id) => {
                tracing::warn!(
                    thread_id,
                    "ignoring app-gateway notification with invalid thread_id"
                );
                return;
            }
            ServerNotificationThreadTarget::Global => {}
        }

        self.chat_widget
            .handle_server_notification(notification, /*replay_kind*/ None);
    }

    async fn handle_server_request_event(
        &mut self,
        app_gateway_client: &AppGatewaySession,
        request: ServerRequest,
    ) {
        if let Some(unsupported) = self
            .pending_app_gateway_requests
            .note_server_request(&request)
        {
            tracing::warn!(
                request_id = ?unsupported.request_id,
                message = unsupported.message,
                "rejecting unsupported app-gateway request"
            );
            self.chat_widget
                .add_error_message(unsupported.message.clone());
            if let Err(err) = self
                .reject_app_gateway_request(
                    app_gateway_client,
                    unsupported.request_id,
                    unsupported.message,
                )
                .await
            {
                tracing::warn!("{err}");
            }
            return;
        }

        let Some(thread_id) = server_request_thread_id(&request) else {
            tracing::warn!("ignoring threadless app-gateway request");
            return;
        };

        let result =
            if self.primary_thread_id == Some(thread_id) || self.primary_thread_id.is_none() {
                self.enqueue_primary_thread_request(request).await
            } else {
                self.enqueue_thread_request(thread_id, request).await
            };
        if let Err(err) = result {
            tracing::warn!("failed to enqueue app-gateway request: {err}");
        }
    }
    async fn reject_app_gateway_request(
        &self,
        app_gateway_client: &AppGatewaySession,
        request_id: praxis_app_gateway_protocol::RequestId,
        reason: String,
    ) -> std::result::Result<(), String> {
        app_gateway_client
            .reject_server_request(
                request_id,
                JSONRPCErrorError {
                    code: -32000,
                    message: reason,
                    data: None,
                },
            )
            .await
            .map_err(|err| format!("failed to reject app-gateway request: {err}"))
    }

    fn workspace_observable_notification_thread_id(
        &self,
        notification: &ServerNotification,
    ) -> Option<ThreadId> {
        match notification {
            ServerNotification::ThreadStarted(notification) => {
                parse_app_gateway_thread_id(&notification.thread.id)
            }
            ServerNotification::ThreadStatusChanged(_)
            | ServerNotification::ThreadControlChanged(_) => {
                server_notification_thread_id(notification)
                    .and_then(parse_app_gateway_thread_id)
                    .filter(|thread_id| self.workspace_thread_should_auto_observe(*thread_id))
            }
            _ => None,
        }
    }
}

fn parse_app_gateway_thread_id(thread_id: &str) -> Option<ThreadId> {
    ThreadId::from_string(thread_id).ok()
}

fn server_request_thread_id(request: &ServerRequest) -> Option<ThreadId> {
    let thread_id = match request {
        ServerRequest::CommandExecutionRequestApproval { params, .. } => {
            Some(params.thread_id.as_str())
        }
        ServerRequest::FileChangeRequestApproval { params, .. } => Some(params.thread_id.as_str()),
        ServerRequest::ToolRequestUserInput { params, .. } => Some(params.thread_id.as_str()),
        ServerRequest::McpServerElicitationRequest { params, .. } => {
            Some(params.thread_id.as_str())
        }
        ServerRequest::PermissionsRequestApproval { params, .. } => Some(params.thread_id.as_str()),
        ServerRequest::DynamicToolCall { params, .. } => Some(params.thread_id.as_str()),
        ServerRequest::ChatgptAuthTokensRefresh { .. } => None,
    }?;
    parse_app_gateway_thread_id(thread_id)
}

#[derive(Debug, PartialEq, Eq)]
enum ServerNotificationThreadTarget {
    Thread(ThreadId),
    InvalidThreadId(String),
    Global,
}

fn server_notification_thread_target(
    notification: &ServerNotification,
) -> ServerNotificationThreadTarget {
    match server_notification_thread_id(notification) {
        Some(thread_id) => match parse_app_gateway_thread_id(thread_id) {
            Some(thread_id) => ServerNotificationThreadTarget::Thread(thread_id),
            None => ServerNotificationThreadTarget::InvalidThreadId(thread_id.to_string()),
        },
        None => ServerNotificationThreadTarget::Global,
    }
}

fn server_notification_thread_id(notification: &ServerNotification) -> Option<&str> {
    match notification {
        ServerNotification::Error(notification) => Some(notification.thread_id.as_str()),
        ServerNotification::ThreadStarted(notification) => Some(notification.thread.id.as_str()),
        ServerNotification::ThreadStatusChanged(notification) => {
            Some(notification.thread_id.as_str())
        }
        ServerNotification::ThreadArchived(notification) => Some(notification.thread_id.as_str()),
        ServerNotification::ThreadUnarchived(notification) => Some(notification.thread_id.as_str()),
        ServerNotification::ThreadClosed(notification) => Some(notification.thread_id.as_str()),
        ServerNotification::ThreadNameUpdated(notification) => {
            Some(notification.thread_id.as_str())
        }
        ServerNotification::ThreadTokenUsageUpdated(notification) => {
            Some(notification.thread_id.as_str())
        }
        ServerNotification::ThreadControlChanged(notification) => {
            Some(notification.thread_id.as_str())
        }
        ServerNotification::ThreadGoalUpdated(notification) => {
            Some(notification.thread_id.as_str())
        }
        ServerNotification::ThreadGoalCleared(notification) => {
            Some(notification.thread_id.as_str())
        }
        ServerNotification::ThreadHeartbeatUpdated(notification) => {
            Some(notification.thread_id.as_str())
        }
        ServerNotification::WorkspaceChangeUpdated(notification) => {
            Some(notification.thread_id.as_str())
        }
        ServerNotification::AutomationRunUpdated(notification) => {
            notification.run.thread_id.as_deref()
        }
        ServerNotification::ThreadModelChanged(notification) => {
            Some(notification.thread_id.as_str())
        }
        ServerNotification::TurnStarted(notification) => Some(notification.thread_id.as_str()),
        ServerNotification::HookStarted(notification) => Some(notification.thread_id.as_str()),
        ServerNotification::TurnCompleted(notification) => Some(notification.thread_id.as_str()),
        ServerNotification::HookCompleted(notification) => Some(notification.thread_id.as_str()),
        ServerNotification::TurnDiffUpdated(notification) => Some(notification.thread_id.as_str()),
        ServerNotification::TurnPlanUpdated(notification) => Some(notification.thread_id.as_str()),
        ServerNotification::ItemStarted(notification) => Some(notification.thread_id.as_str()),
        ServerNotification::ItemGuardianApprovalReviewStarted(notification) => {
            Some(notification.thread_id.as_str())
        }
        ServerNotification::ItemGuardianApprovalReviewCompleted(notification) => {
            Some(notification.thread_id.as_str())
        }
        ServerNotification::ItemCompleted(notification) => Some(notification.thread_id.as_str()),
        ServerNotification::RawResponseItemCompleted(notification) => {
            Some(notification.thread_id.as_str())
        }
        ServerNotification::AgentMessageDelta(notification) => {
            Some(notification.thread_id.as_str())
        }
        ServerNotification::PlanDelta(notification) => Some(notification.thread_id.as_str()),
        ServerNotification::CommandExecutionOutputDelta(notification) => {
            Some(notification.thread_id.as_str())
        }
        ServerNotification::TerminalInteraction(notification) => {
            Some(notification.thread_id.as_str())
        }
        ServerNotification::FileChangeOutputDelta(notification) => {
            Some(notification.thread_id.as_str())
        }
        ServerNotification::ServerRequestResolved(notification) => {
            Some(notification.thread_id.as_str())
        }
        ServerNotification::McpToolCallProgress(notification) => {
            Some(notification.thread_id.as_str())
        }
        ServerNotification::ReasoningSummaryTextDelta(notification) => {
            Some(notification.thread_id.as_str())
        }
        ServerNotification::ReasoningSummaryPartAdded(notification) => {
            Some(notification.thread_id.as_str())
        }
        ServerNotification::ReasoningTextDelta(notification) => {
            Some(notification.thread_id.as_str())
        }
        ServerNotification::ModelRerouted(notification) => Some(notification.thread_id.as_str()),
        ServerNotification::ThreadRealtimeStarted(notification) => {
            Some(notification.thread_id.as_str())
        }
        ServerNotification::ThreadRealtimeItemAdded(notification) => {
            Some(notification.thread_id.as_str())
        }
        ServerNotification::ThreadRealtimeTranscriptUpdated(notification) => {
            Some(notification.thread_id.as_str())
        }
        ServerNotification::ThreadRealtimeOutputAudioDelta(notification) => {
            Some(notification.thread_id.as_str())
        }
        ServerNotification::ThreadRealtimeError(notification) => {
            Some(notification.thread_id.as_str())
        }
        ServerNotification::ThreadRealtimeClosed(notification) => {
            Some(notification.thread_id.as_str())
        }
        ServerNotification::SkillsChanged(_)
        | ServerNotification::McpServerStatusUpdated(_)
        | ServerNotification::McpServerOauthLoginCompleted(_)
        | ServerNotification::AccountUpdated(_)
        | ServerNotification::AccountRateLimitsUpdated(_)
        | ServerNotification::AppListUpdated(_)
        | ServerNotification::DeprecationNotice(_)
        | ServerNotification::ConfigWarning(_)
        | ServerNotification::FuzzyFileSearchSessionUpdated(_)
        | ServerNotification::FuzzyFileSearchSessionCompleted(_)
        | ServerNotification::CommandExecOutputDelta(_)
        | ServerNotification::FsChanged(_)
        | ServerNotification::WindowsWorldWritableWarning(_)
        | ServerNotification::WindowsSandboxSetupCompleted(_)
        | ServerNotification::AccountLoginCompleted(_) => None,
    }
}

#[cfg(test)]
mod tests;
