use super::App;
use super::app_gateway_fetch::build_feedback_upload_params;
use super::app_gateway_fetch::fetch_account_rate_limits;
use super::app_gateway_fetch::fetch_all_mcp_server_statuses;
use super::app_gateway_fetch::fetch_feedback_upload;
use super::app_gateway_fetch::fetch_plugin_command_execute;
use super::app_gateway_fetch::fetch_plugin_detail;
use super::app_gateway_fetch::fetch_plugin_install;
use super::app_gateway_fetch::fetch_plugin_uninstall;
use super::app_gateway_fetch::fetch_plugins_list;
use super::thread_event_store::FeedbackThreadEvent;
use super::thread_event_store::ThreadBufferedEvent;
use crate::app_event::AppEvent;
use crate::app_event::FeedbackCategory;
use crate::app_gateway_session::AppGatewaySession;
use crate::history_cell;
use crate::pager_overlay::Overlay;
use color_eyre::eyre::Result;
use praxis_app_gateway_protocol::McpServerStatus;
use praxis_app_gateway_protocol::PluginReadParams;
use praxis_protocol::ThreadId;
use praxis_utils_absolute_path::AbsolutePathBuf;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::mpsc::error::TrySendError;

impl App {
    /// Spawn a background task that fetches the full MCP server inventory from the
    /// app-gateway via paginated RPCs, then delivers the result back through
    /// `AppEvent::McpInventoryLoaded`.
    ///
    /// The spawned task is fire-and-forget: no `JoinHandle` is stored, so a stale
    /// result may arrive after the user has moved on. We currently accept that
    /// tradeoff because the effect is limited to stale inventory output in history,
    /// while request-token invalidation would add cross-cutting async state for a
    /// low-severity path.
    pub(super) fn fetch_mcp_inventory(&mut self, app_gateway: &AppGatewaySession) {
        let request_handle = app_gateway.request_handle();
        let app_event_tx = self.app_event_tx.clone();
        tokio::spawn(async move {
            let result = fetch_all_mcp_server_statuses(request_handle)
                .await
                .map_err(|err| err.to_string());
            app_event_tx.send(AppEvent::McpInventoryLoaded { result });
        });
    }

    pub(super) fn refresh_rate_limits(&mut self, app_gateway: &AppGatewaySession, request_id: u64) {
        let request_handle = app_gateway.request_handle();
        let app_event_tx = self.app_event_tx.clone();
        tokio::spawn(async move {
            let result = fetch_account_rate_limits(request_handle)
                .await
                .map_err(|err| err.to_string());
            app_event_tx.send(AppEvent::RateLimitsLoaded { request_id, result });
        });
    }

    pub(super) fn fetch_plugins_list(&mut self, app_gateway: &AppGatewaySession, cwd: PathBuf) {
        let request_handle = app_gateway.request_handle();
        let app_event_tx = self.app_event_tx.clone();
        tokio::spawn(async move {
            let result = fetch_plugins_list(request_handle, cwd.clone())
                .await
                .map_err(|err| err.to_string());
            app_event_tx.send(AppEvent::PluginsLoaded { cwd, result });
        });
    }

    pub(super) fn fetch_plugin_detail(
        &mut self,
        app_gateway: &AppGatewaySession,
        cwd: PathBuf,
        params: PluginReadParams,
    ) {
        let request_handle = app_gateway.request_handle();
        let app_event_tx = self.app_event_tx.clone();
        tokio::spawn(async move {
            let result = fetch_plugin_detail(request_handle, params)
                .await
                .map_err(|err| err.to_string());
            app_event_tx.send(AppEvent::PluginDetailLoaded { cwd, result });
        });
    }

    pub(super) fn fetch_plugin_command(
        &mut self,
        app_gateway: &AppGatewaySession,
        command: crate::bottom_pane::PluginCommandInvocation,
    ) {
        let request_handle = app_gateway.request_handle();
        let app_event_tx = self.app_event_tx.clone();
        tokio::spawn(async move {
            let result = fetch_plugin_command_execute(request_handle, &command)
                .await
                .map_err(|err| err.to_string());
            app_event_tx.send(AppEvent::PluginCommandLoaded { command, result });
        });
    }

    pub(super) fn fetch_plugin_install(
        &mut self,
        app_gateway: &AppGatewaySession,
        cwd: PathBuf,
        marketplace_path: AbsolutePathBuf,
        plugin_name: String,
        plugin_display_name: String,
    ) {
        let request_handle = app_gateway.request_handle();
        let app_event_tx = self.app_event_tx.clone();
        tokio::spawn(async move {
            let cwd_for_event = cwd.clone();
            let marketplace_path_for_event = marketplace_path.clone();
            let plugin_name_for_event = plugin_name.clone();
            let result = fetch_plugin_install(request_handle, marketplace_path, plugin_name)
                .await
                .map_err(|err| format!("Failed to install plugin: {err}"));
            app_event_tx.send(AppEvent::PluginInstallLoaded {
                cwd: cwd_for_event,
                marketplace_path: marketplace_path_for_event,
                plugin_name: plugin_name_for_event,
                plugin_display_name,
                result,
            });
        });
    }

    pub(super) fn fetch_plugin_uninstall(
        &mut self,
        app_gateway: &AppGatewaySession,
        cwd: PathBuf,
        plugin_id: String,
        plugin_display_name: String,
    ) {
        let request_handle = app_gateway.request_handle();
        let app_event_tx = self.app_event_tx.clone();
        tokio::spawn(async move {
            let cwd_for_event = cwd.clone();
            let plugin_id_for_event = plugin_id.clone();
            let result = fetch_plugin_uninstall(request_handle, plugin_id)
                .await
                .map_err(|err| format!("Failed to uninstall plugin: {err}"));
            app_event_tx.send(AppEvent::PluginUninstallLoaded {
                cwd: cwd_for_event,
                plugin_id: plugin_id_for_event,
                plugin_display_name,
                result,
            });
        });
    }

    pub(super) fn submit_feedback(
        &mut self,
        app_gateway: &AppGatewaySession,
        category: FeedbackCategory,
        reason: Option<String>,
        include_logs: bool,
    ) {
        let request_handle = app_gateway.request_handle();
        let app_event_tx = self.app_event_tx.clone();
        let origin_thread_id = self.chat_widget.thread_id();
        let rollout_path = if include_logs {
            self.chat_widget.rollout_path()
        } else {
            None
        };
        let params = build_feedback_upload_params(
            origin_thread_id,
            rollout_path,
            category,
            reason,
            include_logs,
        );
        tokio::spawn(async move {
            let result = fetch_feedback_upload(request_handle, params)
                .await
                .map(|response| response.thread_id)
                .map_err(|err| err.to_string());
            app_event_tx.send(AppEvent::FeedbackSubmitted {
                origin_thread_id,
                category,
                include_logs,
                result,
            });
        });
    }

    pub(super) fn handle_feedback_thread_event(&mut self, event: FeedbackThreadEvent) {
        match event.result {
            Ok(thread_id) => {
                self.chat_widget
                    .add_to_history(crate::bottom_pane::feedback_success_cell(
                        event.category,
                        event.include_logs,
                        &thread_id,
                        event.feedback_audience,
                    ))
            }
            Err(err) => self
                .chat_widget
                .add_to_history(history_cell::new_error_event(format!(
                    "Failed to upload feedback: {err}"
                ))),
        }
    }

    pub(super) async fn enqueue_thread_feedback_event(
        &mut self,
        thread_id: ThreadId,
        event: FeedbackThreadEvent,
    ) {
        let (sender, store) = {
            let channel = self.ensure_thread_channel(thread_id);
            (channel.sender.clone(), Arc::clone(&channel.store))
        };

        let should_send = {
            let mut guard = store.lock().await;
            guard.push_feedback_submission(event.clone());
            guard.active
        };

        if should_send {
            match sender.try_send(ThreadBufferedEvent::FeedbackSubmission(event)) {
                Ok(()) => {}
                Err(TrySendError::Full(event)) => {
                    tokio::spawn(async move {
                        if let Err(err) = sender.send(event).await {
                            tracing::warn!("thread {thread_id} event channel closed: {err}");
                        }
                    });
                }
                Err(TrySendError::Closed(_)) => {
                    tracing::warn!("thread {thread_id} event channel closed");
                }
            }
        }
    }

    pub(super) async fn handle_feedback_submitted(
        &mut self,
        origin_thread_id: Option<ThreadId>,
        category: FeedbackCategory,
        include_logs: bool,
        result: Result<String, String>,
    ) {
        let event = FeedbackThreadEvent {
            category,
            include_logs,
            feedback_audience: self.feedback_audience,
            result,
        };
        if let Some(thread_id) = origin_thread_id {
            self.enqueue_thread_feedback_event(thread_id, event).await;
        } else {
            self.handle_feedback_thread_event(event);
        }
    }

    /// Process the completed MCP inventory fetch: clear the loading spinner, then
    /// render either the full tool/resource listing or an error into chat history.
    ///
    /// When both the local config and the app-gateway report zero servers, a special
    /// "empty" cell is shown instead of the full table.
    pub(super) fn handle_mcp_inventory_result(
        &mut self,
        result: Result<Vec<McpServerStatus>, String>,
    ) {
        let config = self.chat_widget.config_ref().clone();
        self.chat_widget.clear_mcp_inventory_loading();
        self.clear_committed_mcp_inventory_loading();

        let statuses = match result {
            Ok(statuses) => statuses,
            Err(err) => {
                self.chat_widget
                    .add_error_message(format!("Failed to load MCP inventory: {err}"));
                return;
            }
        };

        if config.mcp_servers.get().is_empty() && statuses.is_empty() {
            self.chat_widget
                .add_to_history(history_cell::empty_mcp_output());
            return;
        }

        self.chat_widget
            .add_to_history(history_cell::new_mcp_tools_output_from_statuses(
                &config, &statuses,
            ));
    }

    fn clear_committed_mcp_inventory_loading(&mut self) {
        let Some(index) = self
            .transcript_cells
            .iter()
            .rposition(|cell| cell.as_any().is::<history_cell::McpInventoryLoadingCell>())
        else {
            return;
        };

        self.transcript_cells.remove(index);
        if let Some(Overlay::Transcript(overlay)) = &mut self.overlay {
            overlay.replace_cells(self.transcript_cells.clone());
        }
    }
}
