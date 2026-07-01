use super::App;
use super::AppRunControl;
use super::ExitReason;
use super::session_summary;
use super::workspace_view_helpers::goal_status_label;
use super::workspace_view_helpers::truncate_for_goal_notice;
use crate::app_event::AppEvent;
use crate::app_event::ThreadGoalSetMode;
use crate::app_gateway_session::AppGatewaySession;
use crate::cwd_prompt::CwdPromptAction;
use crate::resume_picker::SessionPickerAction;
use crate::resume_picker::SessionSelection;
use crate::tui;
use crate::workspace::SessionPickerOpenRequest;
use crate::workspace::SessionPickerPageRequest;
use color_eyre::eyre::Result;
use praxis_app_gateway_protocol::ThreadGoalStatus;
use praxis_protocol::ThreadId;
use ratatui::style::Stylize;
use ratatui::text::Line;
use tokio::sync::mpsc;

impl App {
    pub(super) async fn open_thread_goal_menu(
        &mut self,
        app_gateway: &mut AppGatewaySession,
        thread_id: ThreadId,
    ) {
        match app_gateway.thread_goal_get(thread_id).await {
            Ok(Some(goal)) => self.chat_widget.show_goal_summary(&goal),
            Ok(None) => self.chat_widget.add_info_message(
                "No goal is set.".to_string(),
                Some("Use /goal <objective> to set this thread goal.".to_string()),
            ),
            Err(err) => self
                .chat_widget
                .add_error_message(format!("Failed to read thread goal: {err:#}")),
        }
    }

    pub(super) async fn open_thread_goal_editor(
        &mut self,
        app_gateway: &mut AppGatewaySession,
        thread_id: Option<ThreadId>,
    ) {
        let Some(thread_id) = thread_id else {
            self.chat_widget
                .add_error_message("No active thread is available.".to_string());
            return;
        };
        match app_gateway.thread_goal_get(thread_id).await {
            Ok(Some(goal)) => self.chat_widget.show_goal_edit_prompt(thread_id, goal),
            Ok(None) => self.chat_widget.add_info_message(
                "No goal is set.".to_string(),
                Some("Use /goal <objective> to set this thread goal first.".to_string()),
            ),
            Err(err) => self
                .chat_widget
                .add_error_message(format!("Failed to load thread goal: {err:#}")),
        }
    }

    pub(super) async fn set_thread_goal_objective(
        &mut self,
        app_gateway: &mut AppGatewaySession,
        thread_id: ThreadId,
        objective: String,
        mode: ThreadGoalSetMode,
    ) {
        let objective = objective.trim().to_string();
        if let Err(err) = praxis_protocol::protocol::validate_thread_goal_objective(&objective) {
            self.chat_widget.add_error_message(err);
            return;
        }

        let result = match mode {
            ThreadGoalSetMode::ReplaceExisting => {
                app_gateway.thread_goal_set(thread_id, objective).await
            }
            ThreadGoalSetMode::UpdateExisting {
                status,
                token_budget,
            } => {
                app_gateway
                    .thread_goal_update(
                        thread_id,
                        Some(objective),
                        Some(status),
                        token_budget,
                        false,
                    )
                    .await
            }
        };

        match result {
            Ok(goal) => self.chat_widget.add_info_message(
                format!(
                    "Goal set: {}",
                    truncate_for_goal_notice(goal.objective.as_str())
                ),
                Some(
                    "Use /goal to inspect it, /goal pause to pause it, or /goal edit to edit it."
                        .to_string(),
                ),
            ),
            Err(err) => self
                .chat_widget
                .add_error_message(format!("Failed to set thread goal: {err:#}")),
        }
    }

    pub(super) async fn set_thread_goal_status(
        &mut self,
        app_gateway: &mut AppGatewaySession,
        thread_id: ThreadId,
        status: ThreadGoalStatus,
    ) {
        match app_gateway
            .thread_goal_update(thread_id, None, Some(status), None, false)
            .await
        {
            Ok(goal) => self.chat_widget.add_info_message(
                format!("Goal status: {}", goal_status_label(goal.status)),
                /*hint*/ None,
            ),
            Err(err) => self
                .chat_widget
                .add_error_message(format!("Failed to update thread goal: {err:#}")),
        }
    }

    pub(super) async fn clear_thread_goal(
        &mut self,
        app_gateway: &mut AppGatewaySession,
        thread_id: ThreadId,
    ) {
        match app_gateway.thread_goal_clear(thread_id).await {
            Ok(true) => {
                self.chat_widget.on_thread_goal_cleared(thread_id);
                self.chat_widget
                    .add_info_message("Goal cleared.".to_string(), /*hint*/ None);
            }
            Ok(false) => self.chat_widget.add_info_message(
                "No goal is set.".to_string(),
                Some("Use /goal <objective> to set this thread goal.".to_string()),
            ),
            Err(err) => self
                .chat_widget
                .add_error_message(format!("Failed to clear thread goal: {err:#}")),
        }
    }

    pub(super) async fn open_thread_picker(
        &mut self,
        tui: &mut tui::Tui,
        app_gateway: &mut AppGatewaySession,
        source: crate::SessionLookupSource,
        action: SessionPickerAction,
    ) -> Result<Option<AppRunControl>> {
        if !self.workspace.enabled
            && let Err(err) = tui.set_mouse_capture_enabled(true)
        {
            tracing::warn!(error = %err, "failed to enable mouse capture for Workspace picker");
        }
        self.workspace.enabled = true;
        self.workspace.clear_session_picker_page_loaders();
        let effect = self
            .workspace
            .open_session_picker(SessionPickerOpenRequest {
                source,
                action,
                include_non_interactive: false,
            });
        self.handle_workspace_main_pane_effect(tui, app_gateway, effect)
            .await
    }

    pub(super) async fn queue_workspace_session_picker_page(
        &mut self,
        request: SessionPickerPageRequest,
    ) {
        let source = request.source;
        if !self.workspace.session_picker_page_loader_is_ready(source) {
            match self.spawn_workspace_session_picker_loader(source).await {
                Ok(sender) => {
                    self.workspace
                        .register_session_picker_page_loader(source, sender);
                }
                Err(err) => {
                    self.app_event_tx
                        .send(AppEvent::WorkspaceSessionPickerPageLoaded {
                            request,
                            result: Err(format!(
                                "Failed to prepare {} picker source: {err}",
                                source.display_name()
                            )),
                        });
                    return;
                }
            }
        }

        if let Some((request, message)) = self.workspace.queue_session_picker_page(request) {
            self.app_event_tx
                .send(AppEvent::WorkspaceSessionPickerPageLoaded {
                    request,
                    result: Err(message),
                });
        }
    }

    async fn spawn_workspace_session_picker_loader(
        &self,
        source: crate::SessionLookupSource,
    ) -> Result<mpsc::UnboundedSender<SessionPickerPageRequest>> {
        let current_gateway_target = match self.remote_app_gateway_url.clone() {
            Some(websocket_url) => crate::AppGatewayTarget::Remote {
                websocket_url,
                auth_token: self.remote_app_gateway_auth_token.clone(),
            },
            None => crate::AppGatewayTarget::Embedded,
        };
        let picker_target =
            crate::session_lookup_app_gateway_target(source, &current_gateway_target);
        let picker_config = crate::build_session_lookup_config(source, &self.config)
            .await
            .map_err(color_eyre::Report::new)?;
        let mut picker_app_gateway =
            crate::start_app_gateway_for_picker(&picker_config, &picker_target).await?;
        let (sender, mut receiver) = mpsc::unbounded_channel::<SessionPickerPageRequest>();
        let app_event_tx = self.app_event_tx.clone();
        tokio::spawn(async move {
            while let Some(request) = receiver.recv().await {
                let params = request.thread_list_params();
                let result = picker_app_gateway
                    .thread_list(params)
                    .await
                    .map_err(|err| err.to_string());
                app_event_tx.send(AppEvent::WorkspaceSessionPickerPageLoaded { request, result });
            }
            if let Err(err) = picker_app_gateway.shutdown().await {
                tracing::warn!(%err, "Failed to shut down Workspace session picker app gateway");
            }
        });
        Ok(sender)
    }

    pub(super) async fn apply_session_selection(
        &mut self,
        tui: &mut tui::Tui,
        app_gateway: &mut AppGatewaySession,
        selection: SessionSelection,
    ) -> Result<Option<AppRunControl>> {
        match selection {
            SessionSelection::Resume(target_session) => {
                if let Some(control) = self
                    .resume_session_target(tui, app_gateway, target_session)
                    .await?
                {
                    return Ok(Some(control));
                }
            }
            SessionSelection::Fork(target_session) => {
                let current_cwd = self.config.cwd.to_path_buf();
                let fork_cwd = if self.remote_app_gateway_url.is_some() {
                    current_cwd.clone()
                } else {
                    match crate::resolve_cwd_for_resume_or_fork(
                        tui,
                        &current_cwd,
                        target_session.cwd.as_deref(),
                        CwdPromptAction::Fork,
                        /*allow_prompt*/ true,
                    )
                    .await?
                    {
                        crate::ResolveCwdOutcome::Continue(Some(cwd)) => cwd,
                        crate::ResolveCwdOutcome::Continue(None) => current_cwd.clone(),
                        crate::ResolveCwdOutcome::Exit => {
                            return Ok(Some(AppRunControl::Exit(ExitReason::UserRequested)));
                        }
                    }
                };
                let (mut fork_config, fork_tui_config) = match self
                    .rebuild_config_for_resume_or_fallback(&current_cwd, fork_cwd)
                    .await
                {
                    Ok(cfg) => cfg,
                    Err(err) => {
                        self.chat_widget.add_error_message(format!(
                            "Failed to rebuild configuration for fork: {err}"
                        ));
                        return Ok(None);
                    }
                };
                self.apply_runtime_policy_overrides(&mut fork_config);
                let summary = session_summary(
                    self.chat_widget.token_usage(),
                    self.chat_widget.thread_id(),
                    self.chat_widget.thread_name(),
                );
                match app_gateway
                    .fork_thread(
                        fork_config.clone(),
                        target_session.thread_id,
                        target_session.path.clone(),
                    )
                    .await
                {
                    Ok(mut forked) => {
                        if forked.session.thread_name.is_none()
                            && let Some(source_name) = target_session.thread_name.as_deref()
                        {
                            match app_gateway
                                .thread_set_name(forked.session.thread_id, source_name.to_string())
                                .await
                            {
                                Ok(()) => {
                                    forked.session.thread_name = Some(source_name.to_string());
                                }
                                Err(err) => {
                                    tracing::warn!(
                                        thread_id = %forked.session.thread_id,
                                        %err,
                                        "Failed to preserve source thread name on in-app fork"
                                    );
                                }
                            }
                        }

                        self.shutdown_current_thread(app_gateway).await;
                        self.config = fork_config;
                        self.tui_config = fork_tui_config;
                        tui.set_notification_method(self.tui_config.notification_method);
                        self.file_search
                            .update_search_dir(self.config.cwd.to_path_buf());
                        match self
                            .replace_chat_widget_with_app_gateway_thread(tui, app_gateway, forked)
                            .await
                        {
                            Ok(()) => {
                                if let Some(summary) = summary {
                                    let mut lines: Vec<Line<'static>> =
                                        vec![summary.usage_line.clone().into()];
                                    if let Some(command) = summary.resume_command {
                                        let spans = vec![
                                            "To continue this session, run ".into(),
                                            command.cyan(),
                                        ];
                                        lines.push(spans.into());
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
                        let path_display = target_session.display_label();
                        self.chat_widget.add_error_message(format!(
                            "Failed to fork session from {path_display}: {err}"
                        ));
                    }
                }
            }
            SessionSelection::Exit | SessionSelection::StartFresh => {}
        }

        self.refresh_workspace_threads(app_gateway, true);
        tui.frame_requester().schedule_frame();
        Ok(None)
    }
}
