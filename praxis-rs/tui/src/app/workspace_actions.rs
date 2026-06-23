use super::App;
use super::AppRunControl;
use super::session_summary;
use crate::app_gateway_session::AppGatewaySession;
use crate::resume_picker::SessionTarget;
use crate::tui;
use crate::ui_language::UiLanguage;
use crate::workspace::WorkspaceArchiveConfirmState;
use crate::workspace::WorkspaceChromeAction;
use crate::workspace::WorkspaceDeleteConfirmState;
use crate::workspace::WorkspaceMenuAction;
use crate::workspace::WorkspaceOpenFolderState;
use crate::workspace::WorkspaceOverlay;
use crate::workspace::WorkspaceRenameState;
use crate::workspace::workspace_menu_action_disabled;
use crate::workspace::workspace_row_is_controlled;
use crate::workspace::workspace_single_line;
use crate::workspace::workspace_status_without_control;
use color_eyre::eyre::Result;
use praxis_protocol::ThreadId;
use ratatui::style::Stylize;
use ratatui::text::Line;
use std::path::PathBuf;

impl App {
    pub(super) async fn execute_workspace_chrome_action(
        &mut self,
        tui: &mut tui::Tui,
        app_gateway: &mut AppGatewaySession,
        action: WorkspaceChromeAction,
    ) -> Result<Option<AppRunControl>> {
        match action {
            WorkspaceChromeAction::NewChat => {
                self.workspace.clear_overlay();
                self.workspace.clear_search_focus();
                self.start_fresh_session_with_summary_hint(tui, app_gateway)
                    .await;
                self.refresh_workspace_threads(app_gateway, true);
                Ok(None)
            }
            WorkspaceChromeAction::OpenFolder => {
                self.workspace.clear_search_focus();
                let value = self.config.cwd.display().to_string();
                let cursor = value.len();
                self.workspace.overlay = WorkspaceOverlay::OpenFolder(WorkspaceOpenFolderState {
                    value,
                    cursor,
                    message: None,
                    area: None,
                });
                Ok(None)
            }
            WorkspaceChromeAction::HelpWebsite => {
                self.workspace.clear_overlay();
                let language = self.chat_widget.ui_language();
                let message = match language {
                    UiLanguage::En => "Cunning3D website is not configured yet.",
                    UiLanguage::Cn => "Cunning3D 官网入口还没有配置。",
                };
                self.chat_widget.add_info_message(message.to_string(), None);
                Ok(None)
            }
        }
    }

    pub(super) async fn commit_workspace_open_folder(
        &mut self,
        tui: &mut tui::Tui,
        app_gateway: &mut AppGatewaySession,
    ) -> Result<Option<AppRunControl>> {
        let value = match self.workspace.overlay.clone() {
            WorkspaceOverlay::OpenFolder(prompt) => prompt.value,
            _ => return Ok(None),
        };
        let raw = value.trim();
        if raw.is_empty() {
            self.set_workspace_open_folder_message("Path cannot be empty.".to_string());
            return Ok(None);
        }
        let path = PathBuf::from(raw);
        let resolved = if path.is_absolute() {
            path
        } else {
            self.config.cwd.as_path().join(path)
        };
        let cwd = match std::fs::canonicalize(&resolved) {
            Ok(path) => path,
            Err(err) => {
                self.set_workspace_open_folder_message(format!("Cannot open folder: {err}"));
                return Ok(None);
            }
        };
        if !cwd.is_dir() {
            self.set_workspace_open_folder_message("Path is not a folder.".to_string());
            return Ok(None);
        }
        let (mut config, tui_config) = match self.rebuild_config_for_cwd(cwd.clone()).await {
            Ok(config) => config,
            Err(err) => {
                self.set_workspace_open_folder_message(format!("Failed to load workspace: {err}"));
                return Ok(None);
            }
        };
        self.apply_runtime_policy_overrides(&mut config);
        self.config = config;
        self.tui_config = tui_config;
        tui.set_notification_method(self.tui_config.notification_method);
        self.chat_widget.sync_plugin_mentions_config(&self.config);
        self.chat_widget.set_tui_config(self.tui_config.clone());
        self.file_search
            .update_search_dir(self.config.cwd.to_path_buf());
        self.workspace.clear_overlay();
        self.start_fresh_session_with_summary_hint(tui, app_gateway)
            .await;
        self.refresh_workspace_threads(app_gateway, true);
        Ok(None)
    }

    fn set_workspace_open_folder_message(&mut self, message: String) {
        if let WorkspaceOverlay::OpenFolder(prompt) = &mut self.workspace.overlay {
            prompt.message = Some(message);
        }
    }

    pub(super) async fn execute_workspace_menu_action(
        &mut self,
        tui: &mut tui::Tui,
        app_gateway: &mut AppGatewaySession,
        action: WorkspaceMenuAction,
    ) -> Result<Option<AppRunControl>> {
        let thread_id = match self.workspace.overlay.clone() {
            WorkspaceOverlay::ContextMenu(menu) => menu.thread_id,
            _ => return Ok(None),
        };
        let locked = self
            .workspace
            .rows
            .iter()
            .find(|row| row.thread_id == thread_id)
            .is_some_and(workspace_row_is_controlled);
        if workspace_menu_action_disabled(action, locked) {
            self.workspace.clear_overlay();
            self.chat_widget.add_info_message(
                "This thread is controlled by another agent.".to_string(),
                Some("Open it and type /release-thread before changing it.".to_string()),
            );
            return Ok(None);
        }
        match action {
            WorkspaceMenuAction::Open => {
                self.workspace.clear_overlay();
                let Some(row) = self
                    .workspace
                    .rows
                    .iter()
                    .find(|row| row.thread_id == thread_id)
                    .cloned()
                else {
                    return Ok(None);
                };
                self.resume_session_target(
                    tui,
                    app_gateway,
                    SessionTarget {
                        path: row.path.clone(),
                        thread_id: row.thread_id,
                        thread_name: Some(row.name.clone()),
                        cwd: Some(row.cwd.clone()),
                    },
                )
                .await
            }
            WorkspaceMenuAction::TogglePin => {
                if !self.workspace.pinned_thread_ids.insert(thread_id) {
                    self.workspace.pinned_thread_ids.remove(&thread_id);
                }
                self.workspace.clear_overlay();
                self.resort_workspace_thread_rows();
                Ok(None)
            }
            WorkspaceMenuAction::Rename => {
                let name = self
                    .workspace
                    .rows
                    .iter()
                    .find(|row| row.thread_id == thread_id)
                    .map(|row| row.name.clone())
                    .unwrap_or_default();
                let cursor = name.len();
                self.workspace.overlay = WorkspaceOverlay::Rename(WorkspaceRenameState {
                    thread_id,
                    value: name,
                    cursor,
                    area: None,
                });
                Ok(None)
            }
            WorkspaceMenuAction::Archive => {
                self.workspace.overlay =
                    WorkspaceOverlay::ConfirmArchive(WorkspaceArchiveConfirmState {
                        thread_id,
                        area: None,
                    });
                Ok(None)
            }
            WorkspaceMenuAction::Delete => {
                self.workspace.overlay =
                    WorkspaceOverlay::ConfirmDelete(WorkspaceDeleteConfirmState {
                        thread_id,
                        area: None,
                    });
                Ok(None)
            }
            WorkspaceMenuAction::ForkLocal => {
                self.workspace.clear_overlay();
                self.fork_workspace_thread_to_local(tui, app_gateway, thread_id)
                    .await;
                Ok(None)
            }
            WorkspaceMenuAction::CopyThreadId => {
                self.workspace.clear_overlay();
                let text = thread_id.to_string();
                match crate::clipboard_text::copy_text_to_clipboard(&text) {
                    Ok(()) => self
                        .chat_widget
                        .add_info_message("Copied thread id to clipboard.".to_string(), None),
                    Err(err) => self
                        .chat_widget
                        .add_error_message(format!("Failed to copy thread id: {err}")),
                }
                Ok(None)
            }
        }
    }

    pub(super) async fn release_workspace_thread_control(
        &mut self,
        app_gateway: &mut AppGatewaySession,
        thread_id: ThreadId,
    ) {
        let row_index = self.workspace.row_index(thread_id);
        if row_index
            .and_then(|index| self.workspace.rows.get(index))
            .is_some_and(|row| !workspace_row_is_controlled(row))
        {
            self.chat_widget
                .add_info_message("This thread is not locked.".to_string(), /*hint*/ None);
            return;
        }

        match app_gateway.thread_control_release(thread_id).await {
            Ok(Some(_previous_control_state)) => {
                if self.chat_widget.thread_id() == Some(thread_id) {
                    self.chat_widget.set_thread_control_state(None);
                }
                self.update_workspace_thread_row(
                    thread_id,
                    /*resort_after_update*/ true,
                    |row| {
                        row.control_state = None;
                        row.status = workspace_status_without_control(&row.status);
                    },
                );
                self.refresh_workspace_threads(app_gateway, true);
                self.chat_widget.add_info_message(
                    "Released the thread lock.".to_string(),
                    Some(
                        "External or agent group controllers can acquire it again on their next action."
                            .to_string(),
                    ),
                );
            }
            Ok(None) => {
                self.chat_widget
                    .add_info_message("This thread is not locked.".to_string(), /*hint*/ None);
            }
            Err(err) => {
                self.chat_widget
                    .add_error_message(format!("Failed to release thread lock: {err}"));
            }
        }
    }

    pub(super) async fn commit_workspace_rename(&mut self, app_gateway: &mut AppGatewaySession) {
        let (thread_id, name) = match self.workspace.overlay.clone() {
            WorkspaceOverlay::Rename(rename) => {
                (rename.thread_id, workspace_single_line(rename.value.trim()))
            }
            _ => return,
        };
        if name.is_empty() {
            self.workspace.clear_overlay();
            return;
        }
        match app_gateway.thread_set_name(thread_id, name.clone()).await {
            Ok(()) => {
                self.update_workspace_thread_row(
                    thread_id,
                    /*resort_after_update*/ false,
                    |row| {
                        row.name = name;
                    },
                );
                self.workspace.clear_overlay();
            }
            Err(err) => {
                self.chat_widget
                    .add_error_message(format!("Failed to rename thread: {err}"));
            }
        }
        self.refresh_workspace_threads(app_gateway, true);
    }

    pub(super) async fn confirm_workspace_archive(
        &mut self,
        tui: &mut tui::Tui,
        app_gateway: &mut AppGatewaySession,
    ) {
        let thread_id = match self.workspace.overlay.clone() {
            WorkspaceOverlay::ConfirmArchive(confirm) => confirm.thread_id,
            _ => return,
        };
        self.workspace.clear_overlay();
        match app_gateway.thread_archive(thread_id).await {
            Ok(()) => {
                self.workspace.pinned_thread_ids.remove(&thread_id);
                self.remove_workspace_thread_row(thread_id);
                if self.workspace_active_thread_id() == Some(thread_id) {
                    self.start_fresh_session_with_summary_hint(tui, app_gateway)
                        .await;
                }
            }
            Err(err) => {
                self.chat_widget
                    .add_error_message(format!("Failed to archive thread: {err}"));
            }
        }
        self.refresh_workspace_threads(app_gateway, true);
    }

    pub(super) async fn confirm_workspace_delete(
        &mut self,
        tui: &mut tui::Tui,
        app_gateway: &mut AppGatewaySession,
    ) {
        let thread_id = match self.workspace.overlay.clone() {
            WorkspaceOverlay::ConfirmDelete(confirm) => confirm.thread_id,
            _ => return,
        };
        self.workspace.clear_overlay();
        match app_gateway.thread_delete(thread_id).await {
            Ok(()) => {
                self.workspace.pinned_thread_ids.remove(&thread_id);
                self.workspace.usage_by_thread.remove(&thread_id);
                self.workspace_observed_thread_ids.remove(&thread_id);
                self.remove_workspace_thread_row(thread_id);
                if self.workspace_active_thread_id() == Some(thread_id) {
                    self.start_fresh_session_with_summary_hint(tui, app_gateway)
                        .await;
                }
            }
            Err(err) => {
                self.chat_widget
                    .add_error_message(format!("Failed to delete thread: {err}"));
            }
        }
        self.refresh_workspace_threads(app_gateway, true);
    }

    async fn fork_workspace_thread_to_local(
        &mut self,
        tui: &mut tui::Tui,
        app_gateway: &mut AppGatewaySession,
        thread_id: ThreadId,
    ) {
        let Some(source) = self
            .workspace
            .rows
            .iter()
            .find(|row| row.thread_id == thread_id)
            .cloned()
        else {
            return;
        };
        self.refresh_in_memory_config_from_disk_best_effort("forking a Praxis thread")
            .await;
        let summary = session_summary(
            self.chat_widget.token_usage(),
            self.chat_widget.thread_id(),
            self.chat_widget.thread_name(),
        );
        match app_gateway
            .fork_thread(self.config.clone(), thread_id, source.path.clone())
            .await
        {
            Ok(mut forked) => {
                if forked.session.thread_name.is_none() {
                    match app_gateway
                        .thread_set_name(forked.session.thread_id, source.name.clone())
                        .await
                    {
                        Ok(()) => forked.session.thread_name = Some(source.name),
                        Err(err) => {
                            tracing::warn!(
                                thread_id = %forked.session.thread_id,
                                %err,
                                "failed to preserve Praxis fork source name"
                            );
                        }
                    }
                }
                self.shutdown_current_thread(app_gateway).await;
                match self
                    .replace_chat_widget_with_app_gateway_thread(tui, app_gateway, forked)
                    .await
                {
                    Ok(()) => {
                        if let Some(summary) = summary {
                            let mut lines: Vec<Line<'static>> =
                                vec![summary.usage_line.clone().into()];
                            if let Some(command) = summary.resume_command {
                                lines.push(
                                    vec!["To continue this session, run ".into(), command.cyan()]
                                        .into(),
                                );
                            }
                            self.chat_widget.add_plain_history_lines(lines);
                        }
                    }
                    Err(err) => {
                        self.chat_widget.add_error_message(format!(
                            "Failed to attach to forked app-gateway thread: {err}"
                        ));
                    }
                }
            }
            Err(err) => {
                self.chat_widget
                    .add_error_message(format!("Failed to fork Praxis thread: {err}"));
            }
        }
        self.refresh_workspace_threads(app_gateway, true);
        tui.frame_requester().schedule_frame();
    }
}
