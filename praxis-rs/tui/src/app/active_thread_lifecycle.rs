use super::*;

#[derive(Clone, Copy)]
enum ThreadViewPresentation {
    Fresh,
    Existing,
}

impl App {
    pub(super) async fn refresh_live_thread_snapshot(
        &mut self,
        app_gateway: &mut AppGatewaySession,
        thread_id: ThreadId,
        is_replay_only: bool,
        snapshot: &mut ThreadEventSnapshot,
    ) -> bool {
        if is_replay_only {
            return true;
        }

        match app_gateway.attach_thread(&self.config, thread_id).await {
            Ok(started) => {
                self.apply_refreshed_snapshot_thread(thread_id, started, snapshot)
                    .await;
                true
            }
            Err(err) => {
                tracing::warn!(
                    thread_id = %thread_id,
                    error = %err,
                    "failed to refresh inferred thread session before replay"
                );
                false
            }
        }
    }

    pub(super) async fn apply_refreshed_snapshot_thread(
        &mut self,
        thread_id: ThreadId,
        started: AppGatewayStartedThread,
        snapshot: &mut ThreadEventSnapshot,
    ) {
        let AppGatewayStartedThread {
            mut session,
            turns,
            status,
            control_state,
        } = started;
        self.apply_current_permissions_to_thread_session(&mut session);
        self.update_workspace_thread_row(thread_id, /*resort_after_update*/ true, |row| {
            row.status = status.clone();
            row.control_state = control_state.clone();
        });
        if let Some(channel) = self.thread_event_channels.get(&thread_id) {
            let mut store = channel.store.lock().await;
            store.set_runtime_snapshot(session, turns, status, control_state);
            store.rebase_buffer_after_session_refresh();
            *snapshot = store.snapshot();
            return;
        }
        snapshot.session = Some(session);
        snapshot.turns = turns;
        snapshot.status = Some(status);
        snapshot.control_state = control_state;
    }

    /// Opens the `/agent` picker after refreshing cached labels for known threads.
    ///
    /// The picker state is derived from long-lived thread channels plus best-effort metadata
    /// refreshes from the backend. Refresh failures are treated as "thread is only inspectable by
    /// historical id now" and converted into closed picker entries instead of deleting them, so
    /// the stable traversal order remains intact for review and keyboard navigation.
    pub(super) fn reset_for_thread_switch(&mut self, tui: &mut tui::Tui) -> Result<()> {
        tui.clear_pending_history_lines();
        self.reset_thread_view_state();
        if self.workspace.enabled {
            return Ok(());
        }
        tui.terminal.clear_scrollback()?;
        tui.terminal.clear()?;
        Ok(())
    }

    pub(super) fn reset_thread_event_state(&mut self) {
        self.abort_all_thread_event_listeners();
        self.thread_event_channels.clear();
        self.agent_navigation.clear();
        self.active_thread_id = None;
        self.active_thread_rx = None;
        self.primary_thread_id = None;
        self.last_subagent_backfill_attempt = None;
        self.primary_session_configured = None;
        self.pending_primary_events.clear();
        self.pending_app_gateway_requests.clear();
        self.workspace_observed_thread_ids.clear();
        self.chat_widget.set_pending_thread_approvals(Vec::new());
        self.sync_active_agent_label();
    }

    pub(super) async fn start_fresh_session(
        &mut self,
        tui: &mut tui::Tui,
        app_gateway: &mut AppGatewaySession,
    ) {
        // Start and select a new thread without interrupting the previously viewed thread.
        self.refresh_in_memory_config_from_disk_best_effort("starting a new thread")
            .await;
        let model = self.chat_widget.current_model().to_string();
        let config = self.fresh_session_config();
        let queued_input_for_lazy_thread = self
            .chat_widget
            .thread_id()
            .is_none()
            .then(|| self.chat_widget.capture_thread_input_state())
            .flatten();
        self.config = config.clone();
        match app_gateway.start_thread(&config).await {
            Ok(started) => {
                if let Err(err) = self
                    .switch_to_fresh_app_gateway_thread_preserving_background(
                        tui,
                        app_gateway,
                        started,
                    )
                    .await
                {
                    self.chat_widget.add_error_message(format!(
                        "Failed to switch to fresh app-gateway thread: {err}"
                    ));
                } else {
                    if let Some(input_state) = queued_input_for_lazy_thread {
                        self.chat_widget
                            .restore_thread_input_state(Some(input_state));
                        self.chat_widget.maybe_send_next_queued_input();
                    }
                }
            }
            Err(err) => {
                self.chat_widget.add_error_message(format!(
                    "Failed to start a fresh session through the app gateway: {err}"
                ));
                self.config.model = Some(model);
            }
        }
        tui.frame_requester().schedule_frame();
    }

    pub(super) async fn switch_to_app_gateway_thread_preserving_background(
        &mut self,
        tui: &mut tui::Tui,
        app_gateway: &mut AppGatewaySession,
        started: AppGatewayStartedThread,
    ) -> Result<()> {
        self.switch_to_app_gateway_thread_preserving_background_kind(
            tui,
            app_gateway,
            started,
            ThreadViewPresentation::Existing,
        )
        .await
    }

    async fn switch_to_fresh_app_gateway_thread_preserving_background(
        &mut self,
        tui: &mut tui::Tui,
        app_gateway: &mut AppGatewaySession,
        started: AppGatewayStartedThread,
    ) -> Result<()> {
        self.switch_to_app_gateway_thread_preserving_background_kind(
            tui,
            app_gateway,
            started,
            ThreadViewPresentation::Fresh,
        )
        .await
    }

    async fn switch_to_app_gateway_thread_preserving_background_kind(
        &mut self,
        tui: &mut tui::Tui,
        app_gateway: &mut AppGatewaySession,
        started: AppGatewayStartedThread,
        presentation: ThreadViewPresentation,
    ) -> Result<()> {
        let AppGatewayStartedThread {
            mut session,
            turns,
            status,
            control_state,
        } = started;
        let previous_thread_id = self.active_thread_id;
        self.store_active_thread_receiver().await;

        self.apply_current_permissions_to_thread_session(&mut session);
        let thread_id = session.thread_id;
        let default_launch_rank = self
            .workspace
            .rows
            .iter()
            .find(|row| row.thread_id == thread_id)
            .map(|row| row.source_kind.agent_rank())
            .unwrap_or(0);
        self.primary_thread_id = Some(thread_id);
        self.primary_session_configured = Some(session.clone());
        self.upsert_agent_picker_thread(
            thread_id, /*agent_base_name*/ None, /*agent_title*/ None,
            /*agent_display_name*/ None, /*agent_role*/ None, /*is_closed*/ false,
        );
        self.update_workspace_thread_row(thread_id, /*resort_after_update*/ true, |row| {
            row.status = status.clone();
            row.control_state = control_state.clone();
        });

        let store = {
            let channel = self.ensure_thread_channel(thread_id);
            Arc::clone(&channel.store)
        };
        {
            let mut store = store.lock().await;
            store.set_runtime_snapshot(session, turns, status, control_state);
            store.rebase_buffer_after_session_refresh();
        }

        let Some((receiver, snapshot)) = self.activate_thread_for_replay(thread_id).await else {
            if let Some(previous_thread_id) = previous_thread_id {
                self.active_thread_id = None;
                self.activate_thread_channel(previous_thread_id).await;
            }
            return Err(color_eyre::eyre::eyre!(
                "Praxis thread {thread_id} is already attached but has no replay receiver."
            ));
        };
        self.active_thread_id = Some(thread_id);
        self.active_thread_rx = Some(receiver);
        self.workspace
            .launch
            .activate_thread(thread_id, default_launch_rank);

        self.advance_history_view_generation();
        let init = match presentation {
            ThreadViewPresentation::Fresh => self.chatwidget_init_for_fresh_thread(
                tui,
                self.config.clone(),
                self.tui_config.clone(),
            ),
            ThreadViewPresentation::Existing => self.chatwidget_init_for_forked_or_resumed_thread(
                tui,
                self.config.clone(),
                self.tui_config.clone(),
            ),
        };
        self.replace_chat_widget(ChatWidget::new_with_app_event(init));
        self.reset_for_thread_switch(tui)?;
        self.workspace.reset_chat_scroll();
        self.replay_thread_snapshot(snapshot, /*resume_restored_queue*/ true);
        self.backfill_loaded_subagent_threads(app_gateway).await;
        self.drain_active_thread_events(tui).await?;
        self.refresh_pending_thread_approvals().await;
        Ok(())
    }

    /// Fetches all loaded threads from the app gateway and registers descendants of the primary
    /// thread in the navigation cache and chat widget metadata.
    ///
    /// Called after replacing the chat widget so `/agent` navigation is pre-populated even
    /// if the TUI did not witness the original spawn events.
    ///
    /// The loaded-thread list is fetched page-by-page and the spawn tree is walked by
    /// `find_loaded_subagent_threads_for_primary`. Each discovered subagent is registered via
    /// `upsert_agent_picker_thread`, which writes to both `AgentNavigationState` and the
    /// `ChatWidget` metadata map.
    pub(super) async fn backfill_loaded_subagent_threads(
        &mut self,
        app_gateway: &mut AppGatewaySession,
    ) -> bool {
        let Some(primary_thread_id) = self.primary_thread_id else {
            return false;
        };

        let mut loaded_thread_ids = Vec::new();
        let mut cursor = None;
        loop {
            let response = match app_gateway
                .thread_loaded_list(loaded_thread_list_params(cursor.clone()))
                .await
            {
                Ok(response) => response,
                Err(err) => {
                    tracing::warn!(%err, "failed to list loaded threads for subagent backfill");
                    return false;
                }
            };
            loaded_thread_ids.extend(response.data);
            let Some(next_cursor) = response.next_cursor else {
                break;
            };
            cursor = Some(next_cursor);
        }

        let mut threads = Vec::new();
        let mut had_read_error = false;
        for thread_id in loaded_thread_ids {
            let Ok(thread_id) = ThreadId::from_string(&thread_id) else {
                tracing::warn!("ignoring loaded thread with invalid id during subagent backfill");
                continue;
            };

            if thread_id == primary_thread_id {
                continue;
            }

            match app_gateway
                .thread_read(thread_id, /*include_turns*/ false)
                .await
            {
                Ok(thread) => threads.push(thread),
                Err(err) => {
                    had_read_error = true;
                    tracing::warn!(thread_id = %thread_id, %err, "failed to read loaded thread");
                }
            }
        }

        for thread in find_loaded_subagent_threads_for_primary(threads, primary_thread_id) {
            self.upsert_agent_picker_thread(
                thread.thread_id,
                thread.agent_base_name,
                thread.agent_title,
                thread.agent_display_name,
                thread.agent_role,
                /*is_closed*/ false,
            );
        }

        !had_read_error
    }

    /// Returns the adjacent thread id for keyboard navigation, backfilling from the server if the
    /// local cache has no neighbor.
    ///
    /// Tries the fast path first: ask `AgentNavigationState` directly. If it returns `None` (no
    /// adjacent entry exists, typically because the cache was never populated with remote
    /// subagents), performs a full `backfill_loaded_subagent_threads` and retries. This ensures the
    /// first next/previous keypress in a resumed remote session discovers subagents on demand
    /// without requiring the user to wait for a proactive fetch.
    pub(super) async fn adjacent_thread_id_with_backfill(
        &mut self,
        app_gateway: &mut AppGatewaySession,
        direction: AgentNavigationDirection,
    ) -> Option<ThreadId> {
        let current_thread = self.current_displayed_thread_id();
        if let Some(thread_id) = self
            .agent_navigation
            .adjacent_thread_id(current_thread, direction)
        {
            return Some(thread_id);
        }

        let primary_thread_id = self.primary_thread_id?;
        if self.last_subagent_backfill_attempt == Some(primary_thread_id) {
            return None;
        }

        if self.backfill_loaded_subagent_threads(app_gateway).await {
            self.last_subagent_backfill_attempt = Some(primary_thread_id);
        }
        self.agent_navigation
            .adjacent_thread_id(self.current_displayed_thread_id(), direction)
    }

    pub(super) fn fresh_session_config(&self) -> Config {
        let mut config = self.config.clone();
        config.service_tier = self.chat_widget.current_service_tier();
        config
    }

    pub(super) async fn drain_active_thread_events(&mut self, tui: &mut tui::Tui) -> Result<()> {
        let Some(mut rx) = self.active_thread_rx.take() else {
            return Ok(());
        };

        let mut disconnected = false;
        loop {
            match rx.try_recv() {
                Ok(envelope) => {
                    if self.active_thread_event_is_after_replay(&envelope) {
                        self.handle_thread_event_now(envelope.event);
                    }
                }
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

        if self.has_pending_transcript_scrollback_work() {
            tui.frame_requester().schedule_frame();
        }
        Ok(())
    }

    /// Returns `(closed_thread_id, primary_thread_id)` when a non-primary active
    /// thread has died and we should fail over to the primary thread.
    ///
    /// A user-requested shutdown (`ExitMode::ShutdownFirst`) sets
    /// `pending_shutdown_exit_thread_id`; matching shutdown completions are ignored
    /// here so Ctrl+C-like exits don't accidentally resurrect the main thread.
    ///
    /// Failover is only eligible when all of these are true:
    /// 1. the event is `thread/closed`;
    /// 2. the active thread differs from the primary thread;
    /// 3. the active thread is not the pending shutdown-exit thread.
    pub(super) fn active_non_primary_shutdown_target(
        &self,
        notification: &ServerNotification,
    ) -> Option<(ThreadId, ThreadId)> {
        if !matches!(notification, ServerNotification::ThreadClosed(_)) {
            return None;
        }
        let active_thread_id = self.active_thread_id?;
        let primary_thread_id = self.primary_thread_id?;
        if self.pending_shutdown_exit_thread_id == Some(active_thread_id) {
            return None;
        }
        (active_thread_id != primary_thread_id).then_some((active_thread_id, primary_thread_id))
    }

    pub(super) fn replay_thread_snapshot(
        &mut self,
        snapshot: ThreadEventSnapshot,
        resume_restored_queue: bool,
    ) {
        let ThreadEventSnapshot {
            session,
            turns,
            status,
            control_state,
            events,
            input_state,
        } = snapshot;
        if let Some(session) = session {
            self.chat_widget.handle_thread_session(session);
        }
        self.chat_widget
            .set_queue_autosend_suppressed(/*suppressed*/ true);
        self.chat_widget.restore_thread_input_state(input_state);
        if !turns.is_empty() {
            let turns = compact_visible_replay_turns(turns);
            self.chat_widget
                .replay_thread_turns(turns, ReplayKind::ThreadSnapshot);
        }
        for event in events {
            self.handle_thread_event_replay(event);
        }
        self.chat_widget
            .apply_thread_status_snapshot(status.as_ref());
        self.chat_widget
            .set_thread_control_state(control_state.as_ref());
        self.chat_widget
            .set_queue_autosend_suppressed(/*suppressed*/ false);
        self.chat_widget
            .set_initial_user_message_submit_suppressed(/*suppressed*/ false);
        self.chat_widget.submit_initial_user_message_if_pending();
        if resume_restored_queue {
            self.chat_widget.maybe_send_next_queued_input();
        }
        self.refresh_status_line();
    }

    pub(super) fn should_wait_for_initial_session(session_selection: &SessionSelection) -> bool {
        matches!(
            session_selection,
            SessionSelection::StartFresh | SessionSelection::Exit
        )
    }

    pub(super) fn should_handle_active_thread_events(
        waiting_for_initial_session_configured: bool,
        has_active_thread_receiver: bool,
    ) -> bool {
        has_active_thread_receiver && !waiting_for_initial_session_configured
    }

    pub(super) fn should_stop_waiting_for_initial_session(
        waiting_for_initial_session_configured: bool,
        primary_thread_id: Option<ThreadId>,
    ) -> bool {
        waiting_for_initial_session_configured && primary_thread_id.is_some()
    }
}
