use super::*;

impl App {
    pub(super) fn handle_history_presentation_shortcut(
        &mut self,
        tui: &mut tui::Tui,
        key_event: KeyEvent,
    ) -> bool {
        if key_event.kind != KeyEventKind::Press {
            return false;
        }

        let toggled = match key_event.code {
            KeyCode::F(6) => {
                history_cell::toggle_reasoning_expanded();
                true
            }
            KeyCode::F(7) => {
                history_cell::toggle_tool_output_expanded();
                true
            }
            KeyCode::F(8) => {
                let visible_patch_cell_ids = self.chat_widget.visible_patch_cell_ids();
                history_cell::toggle_visible_diff_cells(&visible_patch_cell_ids)
            }
            _ => false,
        };

        if !toggled {
            return false;
        }

        if let Some(Overlay::Transcript(overlay)) = self.overlay.as_mut() {
            overlay.replace_cells(self.transcript_cells.clone());
        }
        tui.frame_requester().schedule_frame();
        true
    }

    pub(super) async fn resume_session_target(
        &mut self,
        tui: &mut tui::Tui,
        app_gateway: &mut AppGatewaySession,
        target_session: SessionTarget,
    ) -> Result<Option<AppRunControl>> {
        if self.workspace.enabled {
            return self
                .switch_workspace_thread(tui, app_gateway, target_session)
                .await;
        }

        if Some(target_session.thread_id) == self.chat_widget.thread_id().or(self.active_thread_id)
        {
            tui.frame_requester().schedule_frame();
            return Ok(None);
        }

        let current_cwd = self.config.cwd.to_path_buf();
        let resume_cwd = if self.remote_app_gateway_url.is_some() {
            current_cwd.clone()
        } else {
            let allow_cwd_prompt = !self.workspace.enabled;
            match crate::resolve_cwd_for_resume_or_fork(
                tui,
                &current_cwd,
                target_session.cwd.as_deref(),
                CwdPromptAction::Resume,
                allow_cwd_prompt,
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
        let (mut resume_config, resume_tui_config) = match self
            .rebuild_config_for_resume_or_fallback(&current_cwd, resume_cwd)
            .await
        {
            Ok(cfg) => cfg,
            Err(err) => {
                self.chat_widget.add_error_message(format!(
                    "Failed to rebuild configuration for resume: {err}"
                ));
                return Ok(None);
            }
        };
        self.apply_runtime_policy_overrides(&mut resume_config);
        let summary = session_summary(
            self.chat_widget.token_usage(),
            self.chat_widget.thread_id(),
            self.chat_widget.thread_name(),
        );
        match app_gateway
            .attach_thread(&resume_config, target_session.thread_id)
            .await
        {
            Ok(resumed) => {
                self.config = resume_config;
                self.tui_config = resume_tui_config;
                tui.set_notification_method(self.tui_config.notification_method);
                self.file_search
                    .update_search_dir(self.config.cwd.to_path_buf());
                match self
                    .switch_to_app_gateway_thread_preserving_background(tui, app_gateway, resumed)
                    .await
                {
                    Ok(()) => {
                        if let Some(summary) = summary {
                            let mut lines: Vec<Line<'static>> =
                                vec![summary.usage_line.clone().into()];
                            if let Some(command) = summary.resume_command {
                                let spans =
                                    vec!["To continue this session, run ".into(), command.cyan()];
                                lines.push(spans.into());
                            }
                            self.chat_widget.add_plain_history_lines(lines);
                        }
                    }
                    Err(err) => {
                        self.chat_widget.add_error_message(format!(
                            "Failed to switch to resumed app-gateway thread: {err}"
                        ));
                    }
                }
            }
            Err(err) => {
                let path_display = target_session.display_label();
                self.chat_widget.add_error_message(format!(
                    "Failed to resume session from {path_display}: {err}"
                ));
            }
        }

        self.refresh_workspace_threads(app_gateway, true);
        tui.frame_requester().schedule_frame();
        Ok(None)
    }

    pub(super) async fn switch_workspace_thread(
        &mut self,
        tui: &mut tui::Tui,
        app_gateway: &mut AppGatewaySession,
        target_session: SessionTarget,
    ) -> Result<Option<AppRunControl>> {
        if Some(target_session.thread_id) == self.chat_widget.thread_id().or(self.active_thread_id)
        {
            tui.frame_requester().schedule_frame();
            return Ok(None);
        }

        let current_cwd = self.config.cwd.to_path_buf();
        let resume_cwd = if self.remote_app_gateway_url.is_some() {
            current_cwd.clone()
        } else {
            match crate::resolve_cwd_for_resume_or_fork(
                tui,
                &current_cwd,
                target_session.cwd.as_deref(),
                CwdPromptAction::Resume,
                /*allow_prompt*/ false,
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

        let (mut resume_config, resume_tui_config) = match self
            .rebuild_config_for_resume_or_fallback(&current_cwd, resume_cwd)
            .await
        {
            Ok(cfg) => cfg,
            Err(err) => {
                self.chat_widget.add_error_message(format!(
                    "Failed to rebuild configuration for Praxis thread switch: {err}"
                ));
                return Ok(None);
            }
        };
        self.apply_runtime_policy_overrides(&mut resume_config);

        let resumed = match app_gateway
            .attach_thread(&resume_config, target_session.thread_id)
            .await
        {
            Ok(resumed) => resumed,
            Err(err) => {
                let path_display = target_session.display_label();
                self.chat_widget.add_error_message(format!(
                    "Failed to open Praxis thread from {path_display}: {err}"
                ));
                return Ok(None);
            }
        };

        self.config = resume_config;
        self.tui_config = resume_tui_config;
        tui.set_notification_method(self.tui_config.notification_method);
        self.file_search
            .update_search_dir(self.config.cwd.to_path_buf());
        if let Err(err) = self
            .switch_to_app_gateway_thread_preserving_background(tui, app_gateway, resumed)
            .await
        {
            self.chat_widget.add_error_message(format!(
                "Failed to switch Praxis thread without stopping background work: {err}"
            ));
            return Ok(None);
        }
        self.refresh_workspace_threads(app_gateway, true);
        tui.frame_requester().schedule_frame();
        Ok(None)
    }
}
