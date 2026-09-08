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
        if self.workspace.pending_thread_load.target() == Some(&target_session.thread_id) {
            return Ok(None);
        }
        self.workspace.pending_thread_load.clear();
        if Some(target_session.thread_id) == self.chat_widget.thread_id().or(self.active_thread_id)
        {
            tui.frame_requester().schedule_frame();
            return Ok(None);
        }

        self.begin_thread_switch_measurement(target_session.thread_id);
        let has_cached_projection = self
            .thread_event_channels
            .contains_key(&target_session.thread_id);
        // A materialized view carries the exact runtime config under which it was built. Prefer
        // that over list-row metadata: it avoids filesystem canonicalization for path spelling
        // differences and prevents a stale row cwd from forcing an otherwise hot view through
        // config reload. Unknown views retain the existing row-based behavior.
        let current_cwd = self.config.cwd.as_path();
        let cached_view_same_cwd = self
            .cached_thread_view_cwd(target_session.thread_id)
            .map(|cwd| cwd == current_cwd);
        let known_same_cwd = cached_view_same_cwd.unwrap_or_else(|| {
            target_session
                .cwd
                .as_deref()
                .is_none_or(|cwd| cwd == current_cwd)
        });
        if has_cached_projection
            && known_same_cwd
            && self
                .switch_to_cached_workspace_thread(tui, target_session.thread_id)
                .await?
        {
            self.refresh_workspace_threads(app_gateway, /*force*/ false);
            tui.frame_requester().schedule_frame();
            return Ok(None);
        }

        let current_cwd = self.config.cwd.to_path_buf();
        let cached_view_cwd = self
            .cached_thread_view_cwd(target_session.thread_id)
            .map(Path::to_path_buf);
        let target_cwd = cached_view_cwd.as_deref().or(target_session.cwd.as_deref());
        let resume_cwd = if self.remote_app_gateway_url.is_some() {
            current_cwd.clone()
        } else {
            match crate::resolve_cwd_for_resume_or_fork(
                tui,
                &current_cwd,
                target_cwd,
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

        let same_cwd = !crate::cwds_differ(&current_cwd, &resume_cwd);
        if has_cached_projection
            && !known_same_cwd
            && same_cwd
            && self
                .switch_to_cached_workspace_thread(tui, target_session.thread_id)
                .await?
        {
            // The long-lived observers already keep known rows current. A rate-limited list
            // refresh still discovers new rows without issuing thread/list for every view swap.
            self.refresh_workspace_threads(app_gateway, /*force*/ false);
            tui.frame_requester().schedule_frame();
            return Ok(None);
        }

        let cached_runtime_config = (has_cached_projection && !same_cwd)
            .then(|| self.cached_thread_runtime_config(target_session.thread_id))
            .flatten()
            .filter(|(config, _)| config.cwd.as_path() == resume_cwd.as_path());
        let (mut resume_config, mut resume_tui_config) = if let Some(cached) = cached_runtime_config
        {
            cached
        } else {
            match self
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
            }
        };
        self.apply_runtime_policy_overrides(&mut resume_config);

        if has_cached_projection && !same_cwd {
            let previous_config = std::mem::replace(&mut self.config, resume_config);
            let previous_tui_config = std::mem::replace(&mut self.tui_config, resume_tui_config);
            tui.set_notification_method(self.tui_config.notification_method);
            self.file_search
                .update_search_dir(self.config.cwd.to_path_buf());
            let switch_result = self
                .switch_to_cached_workspace_thread(tui, target_session.thread_id)
                .await;
            if matches!(switch_result, Ok(true)) {
                // Cross-cwd cached views retain their observers too; only discovery needs the
                // normal rate-limited refresh cadence.
                self.refresh_workspace_threads(app_gateway, /*force*/ false);
                tui.frame_requester().schedule_frame();
                return Ok(None);
            }
            resume_config = std::mem::replace(&mut self.config, previous_config);
            resume_tui_config = std::mem::replace(&mut self.tui_config, previous_tui_config);
            tui.set_notification_method(self.tui_config.notification_method);
            self.file_search
                .update_search_dir(self.config.cwd.to_path_buf());
            switch_result?;
        }

        self.begin_workspace_thread_attach(
            tui,
            app_gateway,
            target_session,
            resume_config,
            resume_tui_config,
        );
        Ok(None)
    }
}
