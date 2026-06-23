use super::App;
use super::workspace_view_helpers::workspace_search_term;
use crate::app_event::AppEvent;
use crate::app_gateway_session::AppGatewaySession;
use crate::app_gateway_session::AppGatewayStartedThread;
use crate::app_gateway_session::token_usage_info_from_app_gateway;
use crate::workspace::ThreadListRow;
use crate::workspace::parse_workspace_thread_id;
use crate::workspace::sort_workspace_thread_rows;
use crate::workspace::workspace_row_should_auto_observe;
use crate::workspace::workspace_single_line;
use crate::workspace::workspace_thread_list_params;
use crate::workspace::workspace_token_usage_thread_list_params;
use praxis_app_gateway_protocol::ClientRequest;
use praxis_app_gateway_protocol::RequestId;
use praxis_app_gateway_protocol::ServerNotification;
use praxis_app_gateway_protocol::ThreadListResponse;
use praxis_app_gateway_protocol::ThreadStatus;
use praxis_protocol::ThreadId;
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;
use std::time::Instant;
use uuid::Uuid;

const WORKSPACE_REFRESH_INTERVAL: Duration = Duration::from_secs(2);
const WORKSPACE_REFRESH_TIMEOUT: Duration = Duration::from_secs(10);

impl App {
    pub(super) fn workspace_active_thread_id(&self) -> Option<ThreadId> {
        self.chat_widget.thread_id().or(self.active_thread_id)
    }

    pub(super) fn resort_workspace_thread_rows(&mut self) {
        let active_thread_id = self.workspace_active_thread_id();
        let visible_rows = self.workspace_visible_row_capacity();
        sort_workspace_thread_rows(
            &mut self.workspace.rows,
            active_thread_id,
            &self.workspace.pinned_thread_ids,
        );
        self.workspace.clamp_selection(visible_rows);
    }

    fn clamp_workspace_thread_rows(&mut self) {
        let visible_rows = self.workspace_visible_row_capacity();
        self.workspace.clamp_selection(visible_rows);
    }

    pub(super) fn update_workspace_thread_row(
        &mut self,
        thread_id: ThreadId,
        resort_after_update: bool,
        update: impl FnOnce(&mut ThreadListRow),
    ) -> bool {
        let Some(index) = self.workspace.row_index(thread_id) else {
            return false;
        };
        update(&mut self.workspace.rows[index]);
        if resort_after_update {
            self.resort_workspace_thread_rows();
        }
        true
    }

    pub(super) fn remove_workspace_thread_row(&mut self, thread_id: ThreadId) {
        self.workspace.rows.retain(|row| row.thread_id != thread_id);
        self.clamp_workspace_thread_rows();
    }

    fn upsert_workspace_thread_row(&mut self, row: ThreadListRow) {
        if let Some(index) = self.workspace.row_index(row.thread_id) {
            self.workspace.rows[index] = row;
        } else {
            self.workspace.rows.push(row);
        }
        self.resort_workspace_thread_rows();
    }

    pub(super) async fn observe_workspace_thread_if_needed(
        &mut self,
        app_gateway: &mut AppGatewaySession,
        thread_id: ThreadId,
    ) {
        if !self.workspace.enabled {
            return;
        }
        if self.primary_thread_id == Some(thread_id) {
            self.workspace_observed_thread_ids.insert(thread_id);
            return;
        }
        if !self.workspace_observed_thread_ids.insert(thread_id) {
            return;
        }

        match app_gateway.watch_thread(&self.config, thread_id).await {
            Ok(started) => self.apply_workspace_observed_thread(started).await,
            Err(err) => {
                self.workspace_observed_thread_ids.remove(&thread_id);
                tracing::warn!(
                    thread_id = %thread_id,
                    error = %err,
                    "failed to attach Praxis observer to externally started thread"
                );
            }
        }
    }

    pub(super) fn workspace_thread_should_auto_observe(&self, thread_id: ThreadId) -> bool {
        self.workspace
            .row_index(thread_id)
            .and_then(|index| self.workspace.rows.get(index))
            .is_some_and(workspace_row_should_auto_observe)
    }

    pub(super) async fn observe_existing_workspace_threads_if_needed(
        &mut self,
        app_gateway: &mut AppGatewaySession,
    ) {
        if !self.workspace.enabled {
            return;
        }
        let thread_ids = self
            .workspace
            .rows
            .iter()
            .filter(|row| workspace_row_should_auto_observe(row))
            .map(|row| row.thread_id)
            .collect::<Vec<_>>();
        for thread_id in thread_ids {
            self.observe_workspace_thread_if_needed(app_gateway, thread_id)
                .await;
        }
    }

    async fn apply_workspace_observed_thread(&mut self, started: AppGatewayStartedThread) {
        let AppGatewayStartedThread {
            mut session,
            turns,
            status,
            control_state,
        } = started;
        self.apply_current_permissions_to_thread_session(&mut session);
        let thread_id = session.thread_id;
        self.update_workspace_thread_row(thread_id, /*resort_after_update*/ true, |row| {
            row.status = status;
            row.control_state = control_state.clone();
        });

        let store = {
            let channel = self.ensure_thread_channel(thread_id);
            Arc::clone(&channel.store)
        };
        {
            let mut store = store.lock().await;
            store.set_session(session, turns);
            store.rebase_buffer_after_session_refresh();
        }

        if self.workspace_active_thread_id() == Some(thread_id) {
            self.chat_widget
                .set_thread_control_state(control_state.as_ref());
        }
    }

    pub(super) fn apply_workspace_server_notification(
        &mut self,
        notification: &ServerNotification,
    ) -> bool {
        if !self.workspace.enabled {
            return false;
        }

        match notification {
            ServerNotification::ThreadStarted(notification) => {
                let Some(row) = ThreadListRow::from_thread(notification.thread.clone()) else {
                    return true;
                };
                self.upsert_workspace_thread_row(row);
                false
            }
            ServerNotification::ThreadStatusChanged(notification) => {
                let Some(thread_id) = parse_workspace_thread_id(&notification.thread_id) else {
                    return true;
                };
                if !self.update_workspace_thread_row(
                    thread_id,
                    /*resort_after_update*/ true,
                    |row| {
                        row.status = notification.status.clone();
                    },
                ) {
                    return true;
                }
                false
            }
            ServerNotification::ThreadControlChanged(notification) => {
                let Some(thread_id) = parse_workspace_thread_id(&notification.thread_id) else {
                    return true;
                };
                if !self.update_workspace_thread_row(
                    thread_id,
                    /*resort_after_update*/ true,
                    |row| {
                        row.control_state = notification.control_state.clone();
                    },
                ) {
                    return true;
                }
                if self
                    .workspace_active_thread_id()
                    .is_some_and(|active_thread_id| active_thread_id == thread_id)
                {
                    self.chat_widget
                        .set_thread_control_state(notification.control_state.as_ref());
                }
                false
            }
            ServerNotification::ThreadNameUpdated(notification) => {
                let Some(thread_id) = parse_workspace_thread_id(&notification.thread_id) else {
                    return true;
                };
                if let Some(name) = notification.thread_name.as_deref() {
                    if !self.update_workspace_thread_row(
                        thread_id,
                        /*resort_after_update*/ false,
                        |row| {
                            row.name = workspace_single_line(name);
                        },
                    ) {
                        return true;
                    }
                    false
                } else {
                    true
                }
            }
            ServerNotification::ThreadTokenUsageUpdated(notification) => {
                let Some(thread_id) = parse_workspace_thread_id(&notification.thread_id) else {
                    return true;
                };
                let info = token_usage_info_from_app_gateway(notification.token_usage.clone());
                self.workspace
                    .usage_by_thread
                    .insert(thread_id, info.clone());
                self.update_workspace_thread_row(
                    thread_id,
                    /*resort_after_update*/ false,
                    |row| {
                        row.token_usage = Some(info);
                    },
                );
                false
            }
            ServerNotification::ThreadModelChanged(notification) => {
                let Some(thread_id) = parse_workspace_thread_id(&notification.thread_id) else {
                    return true;
                };
                if !self.update_workspace_thread_row(
                    thread_id,
                    /*resort_after_update*/ false,
                    |row| {
                        if row.preview.trim().is_empty()
                            || row.preview == notification.previous_model_provider
                        {
                            row.preview = notification.model_provider.clone();
                        }
                    },
                ) {
                    return true;
                }
                false
            }
            ServerNotification::ThreadArchived(notification) => {
                let Some(thread_id) = parse_workspace_thread_id(&notification.thread_id) else {
                    return true;
                };
                self.remove_workspace_thread_row(thread_id);
                false
            }
            ServerNotification::ThreadUnarchived(_) => true,
            ServerNotification::ThreadClosed(notification) => {
                let Some(thread_id) = parse_workspace_thread_id(&notification.thread_id) else {
                    return true;
                };
                if !self.update_workspace_thread_row(
                    thread_id,
                    /*resort_after_update*/ true,
                    |row| {
                        row.status = ThreadStatus::NotLoaded;
                        row.control_state = None;
                    },
                ) {
                    return true;
                }
                false
            }
            _ => false,
        }
    }

    pub(super) fn refresh_workspace_threads(
        &mut self,
        app_gateway: &AppGatewaySession,
        force: bool,
    ) {
        self.request_workspace_threads(app_gateway, force, None);
    }

    pub(super) fn fetch_token_usage_summary(&self, app_gateway: &AppGatewaySession, limit: usize) {
        let app_event_tx = self.app_event_tx.clone();
        let request_handle = app_gateway.request_handle();
        tokio::spawn(async move {
            let result = tokio::time::timeout(
                WORKSPACE_REFRESH_TIMEOUT,
                request_handle.request_typed::<ThreadListResponse>(ClientRequest::ThreadList {
                    request_id: RequestId::String(format!("praxis-token-usage-{}", Uuid::new_v4())),
                    params: workspace_token_usage_thread_list_params(limit),
                }),
            )
            .await
            .map_err(|_| "thread/list timed out while summarizing token usage".to_string())
            .and_then(|result| result.map_err(|err| err.to_string()));
            app_event_tx.send(AppEvent::TokenUsageSummaryLoaded { limit, result });
        });
    }

    pub(super) fn load_more_workspace_threads(&mut self, app_gateway: &AppGatewaySession) {
        let Some(cursor) = self.workspace.pagination.next_cursor() else {
            return;
        };
        self.request_workspace_threads(app_gateway, true, Some(cursor));
    }

    fn request_workspace_threads(
        &mut self,
        app_gateway: &AppGatewaySession,
        force: bool,
        cursor: Option<String>,
    ) {
        if !self.workspace.enabled {
            return;
        }
        if self.workspace.refresh_in_flight {
            return;
        }
        let is_load_more = cursor.is_some();
        if !force
            && !is_load_more
            && self
                .workspace
                .last_refresh_at
                .is_some_and(|last| last.elapsed() < WORKSPACE_REFRESH_INTERVAL)
        {
            return;
        }
        self.workspace.last_refresh_at = Some(Instant::now());
        self.workspace.refresh_in_flight = true;
        self.workspace.refresh_request_id = self.workspace.refresh_request_id.wrapping_add(1);
        self.workspace.pagination.set_pending_cursor(cursor.clone());
        if !is_load_more {
            self.workspace.pagination.clear_next_cursor();
        }

        let request_id = self.workspace.refresh_request_id;
        let search_term = workspace_search_term(&self.workspace.search_query);
        let app_event_tx = self.app_event_tx.clone();
        let request_handle = app_gateway.request_handle();
        tokio::spawn(async move {
            let result = tokio::time::timeout(
                WORKSPACE_REFRESH_TIMEOUT,
                request_handle.request_typed::<ThreadListResponse>(ClientRequest::ThreadList {
                    request_id: RequestId::String(format!(
                        "praxis-workspace-thread-list-{request_id}"
                    )),
                    params: workspace_thread_list_params(search_term, cursor),
                }),
            )
            .await
            .map_err(|_| "thread/list timed out while refreshing Praxis workspace".to_string())
            .and_then(|result| result.map_err(|err| err.to_string()));
            app_event_tx.send(AppEvent::WorkspaceThreadsLoaded { request_id, result });
        });
    }

    pub(super) fn handle_workspace_threads_loaded(
        &mut self,
        request_id: u64,
        result: std::result::Result<ThreadListResponse, String>,
    ) {
        if request_id != self.workspace.refresh_request_id {
            return;
        }
        let append = self.workspace.pagination.is_pending_next_page();
        self.workspace.refresh_in_flight = false;
        self.workspace.pagination.take_pending_cursor();
        let response = match result {
            Ok(response) => response,
            Err(err) => {
                tracing::warn!(%err, "failed to refresh Praxis thread list");
                return;
            }
        };
        self.apply_workspace_threads(response, append);
    }

    fn apply_workspace_threads(&mut self, response: ThreadListResponse, append: bool) {
        let active_thread_id = self.workspace_active_thread_id();
        let was_empty = self.workspace.rows.is_empty();
        let previous_top_thread_id = self.workspace.top_visible_thread_id();
        let old_len = self.workspace.rows.len();
        let old_visible_len = self.workspace.visible_row_count();
        let selected_was_load_more =
            append && self.workspace.is_load_more_index(self.workspace.selected);
        let mut incoming_rows: Vec<ThreadListRow> = response
            .data
            .into_iter()
            .filter_map(ThreadListRow::from_thread)
            .collect();
        let first_incoming_thread_id = incoming_rows.first().map(|row| row.thread_id);
        let mut rows = if append {
            std::mem::take(&mut self.workspace.rows)
        } else {
            Vec::new()
        };
        for row in incoming_rows.drain(..) {
            if let Some(index) = rows
                .iter()
                .position(|existing| existing.thread_id == row.thread_id)
            {
                rows[index] = row;
            } else {
                rows.push(row);
            }
        }
        sort_workspace_thread_rows(
            &mut rows,
            active_thread_id,
            &self.workspace.pinned_thread_ids,
        );

        let selected_thread_id = self
            .workspace
            .actual_row_index_for_visible(self.workspace.selected)
            .and_then(|index| self.workspace.rows.get(index))
            .map(|row| row.thread_id)
            .or(active_thread_id);
        self.workspace.rows = rows;
        self.workspace
            .pagination
            .set_next_cursor(response.next_cursor);
        let loaded_thread_ids = self
            .workspace
            .rows
            .iter()
            .map(|row| row.thread_id)
            .collect::<HashSet<_>>();
        self.workspace
            .expanded_subagent_parent_ids
            .retain(|thread_id| loaded_thread_ids.contains(thread_id));
        self.workspace
            .expanded_closed_subagent_parent_ids
            .retain(|thread_id| loaded_thread_ids.contains(thread_id));
        self.workspace.reconcile_selection_after_thread_refresh(
            selected_was_load_more,
            old_len,
            old_visible_len,
            first_incoming_thread_id,
            selected_thread_id,
        );
        let visible_rows = self.workspace_visible_row_capacity();
        if visible_rows > 0 {
            if was_empty
                || !self
                    .workspace
                    .keep_top_visible_thread(previous_top_thread_id, visible_rows)
            {
                self.workspace.ensure_selected_visible(visible_rows);
            } else {
                self.workspace.clamp_list_scroll(visible_rows);
            }
        }
    }
}
