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
            .resume_thread(resume_config.clone(), target_session.thread_id)
            .await
        {
            Ok(resumed) => {
                self.shutdown_current_thread(app_gateway).await;
                self.config = resume_config;
                self.tui_config = resume_tui_config;
                tui.set_notification_method(self.tui_config.notification_method);
                self.file_search
                    .update_search_dir(self.config.cwd.to_path_buf());
                match self
                    .replace_chat_widget_with_app_gateway_thread(tui, app_gateway, resumed)
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
                            "Failed to attach to resumed app-gateway thread: {err}"
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
            .resume_thread(resume_config.clone(), target_session.thread_id)
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

        let AppGatewayStartedThread {
            mut session,
            turns,
            status: _,
            control_state,
        } = resumed;
        let previous_thread_id = self.active_thread_id;
        self.store_active_thread_receiver().await;

        self.config = resume_config;
        self.tui_config = resume_tui_config;
        tui.set_notification_method(self.tui_config.notification_method);
        self.file_search
            .update_search_dir(self.config.cwd.to_path_buf());

        self.apply_current_permissions_to_thread_session(&mut session);
        let thread_id = session.thread_id;
        self.primary_thread_id = Some(thread_id);
        self.primary_session_configured = Some(session.clone());
        self.upsert_agent_picker_thread(
            thread_id, /*agent_base_name*/ None, /*agent_title*/ None,
            /*agent_display_name*/ None, /*agent_role*/ None, /*is_closed*/ false,
        );

        let store = {
            let channel = self.ensure_thread_channel(thread_id);
            Arc::clone(&channel.store)
        };
        {
            let mut store = store.lock().await;
            store.set_session(session, turns);
            store.rebase_buffer_after_session_refresh();
        }

        let Some((receiver, snapshot)) = self.activate_thread_for_replay(thread_id).await else {
            if let Some(previous_thread_id) = previous_thread_id {
                self.active_thread_id = None;
                self.activate_thread_channel(previous_thread_id).await;
            }
            self.chat_widget.add_error_message(format!(
                "Praxis thread {thread_id} is already attached but has no replay receiver."
            ));
            return Ok(None);
        };
        self.active_thread_id = Some(thread_id);
        self.active_thread_rx = Some(receiver);

        let init = self.chatwidget_init_for_forked_or_resumed_thread(
            tui,
            self.config.clone(),
            self.tui_config.clone(),
        );
        self.replace_chat_widget(ChatWidget::new_with_app_event(init));
        self.reset_for_thread_switch(tui)?;
        self.workspace.reset_chat_scroll();
        self.replay_thread_snapshot(snapshot, /*resume_restored_queue*/ true);
        self.chat_widget
            .set_thread_control_state(control_state.as_ref());
        self.backfill_loaded_subagent_threads(app_gateway).await;
        self.drain_active_thread_events(tui).await?;
        self.refresh_pending_thread_approvals().await;
        self.refresh_workspace_threads(app_gateway, true);
        tui.frame_requester().schedule_frame();
        Ok(None)
    }
}
