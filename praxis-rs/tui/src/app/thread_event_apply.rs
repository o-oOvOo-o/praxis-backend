use super::*;

impl App {
    pub(super) async fn handle_exit_mode(
        &mut self,
        app_gateway: &mut AppGatewaySession,
        mode: ExitMode,
    ) -> AppRunControl {
        match mode {
            ExitMode::ShutdownFirst => {
                // Mark the thread we are explicitly shutting down for exit so
                // its shutdown completion does not trigger agent failover.
                self.pending_shutdown_exit_thread_id =
                    self.active_thread_id.or(self.chat_widget.thread_id());
                if self.pending_shutdown_exit_thread_id.is_some() {
                    self.shutdown_current_thread(app_gateway).await;
                }
                self.pending_shutdown_exit_thread_id = None;
                AppRunControl::Exit(ExitReason::UserRequested)
            }
            ExitMode::Immediate => {
                self.pending_shutdown_exit_thread_id = None;
                AppRunControl::Exit(ExitReason::UserRequested)
            }
        }
    }

    pub(super) fn handle_skills_list_response(&mut self, response: SkillsListResponse) {
        let response = list_skills_response_to_core(response);
        let cwd = self.chat_widget.config_ref().cwd.clone();
        let errors = errors_for_cwd(&cwd, &response);
        emit_skill_load_warnings(&self.app_event_tx, &errors);
        self.chat_widget.handle_skills_list_response(response);
    }

    pub(super) async fn handle_thread_rollback_response(
        &mut self,
        thread_id: ThreadId,
        num_turns: u32,
        response: &ThreadRollbackResponse,
    ) {
        if let Some(channel) = self.thread_event_channels.get(&thread_id) {
            let mut store = channel.store.lock().await;
            store.apply_thread_rollback(response);
        }
        if self.active_thread_id == Some(thread_id)
            && let Some(mut rx) = self.active_thread_rx.take()
        {
            let mut disconnected = false;
            loop {
                match rx.try_recv() {
                    Ok(_) => {}
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        disconnected = true;
                        break;
                    }
                }
            }

            if !disconnected {
                self.active_thread_rx = Some(rx);
            } else {
                self.clear_active_thread().await;
            }
        }
        self.handle_backtrack_rollback_succeeded(num_turns);
        self.chat_widget.handle_thread_rolled_back();
    }

    pub(super) fn handle_thread_event_now(&mut self, event: ThreadBufferedEvent) {
        let needs_refresh = matches!(
            &event,
            ThreadBufferedEvent::Notification(ServerNotification::TurnStarted(_))
                | ThreadBufferedEvent::Notification(ServerNotification::ThreadTokenUsageUpdated(_))
        );
        self.remember_workspace_token_usage_from_event(&event);
        match event {
            ThreadBufferedEvent::Notification(notification) => {
                if let ServerNotification::ThreadModelChanged(notification) = &notification {
                    self.apply_thread_model_changed_to_runtime_config(notification);
                }
                self.chat_widget
                    .handle_server_notification(notification, /*replay_kind*/ None);
            }
            ThreadBufferedEvent::Request(request) => {
                self.chat_widget
                    .handle_server_request(request, /*replay_kind*/ None);
            }
            ThreadBufferedEvent::HistoryEntryResponse(event) => {
                self.chat_widget.handle_history_entry_response(event);
            }
            ThreadBufferedEvent::FeedbackSubmission(event) => {
                self.handle_feedback_thread_event(event);
            }
        }
        if needs_refresh {
            self.refresh_status_line();
        }
    }

    pub(super) fn handle_thread_event_replay(&mut self, event: ThreadBufferedEvent) {
        self.remember_workspace_token_usage_from_event(&event);
        match event {
            ThreadBufferedEvent::Notification(notification) => {
                if let ServerNotification::ThreadModelChanged(notification) = &notification {
                    self.apply_thread_model_changed_to_runtime_config(notification);
                }
                self.chat_widget
                    .handle_server_notification(notification, Some(ReplayKind::ThreadSnapshot));
            }
            ThreadBufferedEvent::Request(request) => self
                .chat_widget
                .handle_server_request(request, Some(ReplayKind::ThreadSnapshot)),
            ThreadBufferedEvent::HistoryEntryResponse(event) => {
                self.chat_widget.handle_history_entry_response(event)
            }
            ThreadBufferedEvent::FeedbackSubmission(event) => {
                self.handle_feedback_thread_event(event);
            }
        }
    }

    pub(super) fn remember_workspace_token_usage_from_event(
        &mut self,
        event: &ThreadBufferedEvent,
    ) {
        if !self.workspace.enabled {
            return;
        }
        let ThreadBufferedEvent::Notification(ServerNotification::ThreadTokenUsageUpdated(
            notification,
        )) = event
        else {
            return;
        };
        let Some(thread_id) = parse_workspace_thread_id(&notification.thread_id) else {
            return;
        };
        self.workspace.usage_by_thread.insert(
            thread_id,
            token_usage_info_from_app_gateway(notification.token_usage.clone()),
        );
    }

    pub(super) fn apply_thread_model_changed_to_runtime_config(
        &mut self,
        notification: &ThreadModelChangedNotification,
    ) {
        self.config.model_provider_id = notification.model_provider.clone();
        self.config.model = Some(notification.model.clone());
        self.config.model_reasoning_effort = notification.reasoning_effort;
        if let Some(provider) = self
            .config
            .model_providers
            .get(&notification.model_provider)
            .cloned()
        {
            self.config.model_provider = provider;
        }
        if self
            .primary_thread_id
            .is_some_and(|thread_id| thread_id.to_string() == notification.thread_id.as_str())
            && let Some(session) = self.primary_session_configured.as_mut()
        {
            session.model_provider_id = notification.model_provider.clone();
            session.model = notification.model.clone();
            session.reasoning_effort = notification.reasoning_effort;
        }
    }

    /// Handles an event emitted by the currently active thread.
    ///
    /// This function enforces shutdown intent routing: unexpected non-primary
    /// thread shutdowns fail over to the primary thread, while user-requested
    /// app exits consume only the tracked shutdown completion and then proceed.
    pub(super) async fn handle_active_thread_event(
        &mut self,
        tui: &mut tui::Tui,
        app_gateway: &mut AppGatewaySession,
        event: ThreadBufferedEvent,
    ) -> Result<()> {
        // Capture this before any potential thread switch: we only want to clear
        // the exit marker when the currently active thread acknowledges shutdown.
        let pending_shutdown_exit_completed = matches!(
            &event,
            ThreadBufferedEvent::Notification(ServerNotification::ThreadClosed(_))
        ) && self.pending_shutdown_exit_thread_id
            == self.active_thread_id;

        // Processing order matters:
        //
        // 1. handle unexpected non-primary shutdown failover first;
        // 2. clear pending exit marker for matching shutdown;
        // 3. forward the event through normal handling.
        //
        // This preserves the mental model that user-requested exits do not trigger
        // failover, while true sub-agent deaths still do.
        if let ThreadBufferedEvent::Notification(notification) = &event
            && let Some((closed_thread_id, primary_thread_id)) =
                self.active_non_primary_shutdown_target(notification)
        {
            self.mark_agent_picker_thread_closed(closed_thread_id);
            self.select_agent_thread(tui, app_gateway, primary_thread_id)
                .await?;
            if self.active_thread_id == Some(primary_thread_id) {
                self.chat_widget.add_info_message(
                    format!(
                        "Agent thread {closed_thread_id} closed. Switched back to main thread."
                    ),
                    /*hint*/ None,
                );
            } else {
                self.clear_active_thread().await;
                self.chat_widget.add_error_message(format!(
                    "Agent thread {closed_thread_id} closed. Failed to switch back to main thread {primary_thread_id}.",
                ));
            }
            return Ok(());
        }

        if pending_shutdown_exit_completed {
            // Clear only after seeing the shutdown completion for the tracked
            // thread, so unrelated shutdowns cannot consume this marker.
            self.pending_shutdown_exit_thread_id = None;
        }
        if let ThreadBufferedEvent::Notification(notification) = &event {
            self.hydrate_collab_agent_metadata_for_notification(app_gateway, notification)
                .await;
        }

        self.handle_thread_event_now(event);
        if self.has_pending_transcript_scrollback_work() {
            tui.frame_requester().schedule_frame();
        }
        Ok(())
    }
}
