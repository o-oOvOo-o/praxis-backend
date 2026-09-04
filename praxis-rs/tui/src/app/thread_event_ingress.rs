use super::*;

impl App {
    pub(super) fn refresh_pending_thread_approvals(&mut self) {
        #[cfg(test)]
        {
            self.pending_approval_thread_ids = self
                .thread_event_channels
                .iter()
                .filter_map(|(thread_id, channel)| {
                    lock_thread_event_store(&channel.store)
                        .has_pending_thread_approvals()
                        .then_some(*thread_id)
                })
                .collect();
        }
        let mut pending_thread_ids = self
            .pending_approval_thread_ids
            .iter()
            .copied()
            .filter(|thread_id| Some(*thread_id) != self.active_thread_id)
            .collect::<Vec<_>>();
        pending_thread_ids.sort_by_cached_key(ThreadId::to_string);

        let threads = pending_thread_ids
            .into_iter()
            .map(|thread_id| self.thread_label(thread_id))
            .collect();

        self.chat_widget.set_pending_thread_approvals(threads);
    }

    pub(super) fn set_pending_thread_approval_state(
        &mut self,
        thread_id: ThreadId,
        has_pending: bool,
    ) {
        if has_pending {
            self.pending_approval_thread_ids.insert(thread_id);
        } else {
            self.pending_approval_thread_ids.remove(&thread_id);
        }
    }

    pub(super) async fn enqueue_thread_notification(
        &mut self,
        thread_id: ThreadId,
        notification: ServerNotification,
    ) -> Result<()> {
        let inferred_session = self.infer_session_for_thread_notification(thread_id, &notification);
        let store = Arc::clone(&self.ensure_thread_channel(thread_id).store);

        let (live_notification, sequence, pending_approval_visibility_changed, has_pending) = {
            let mut guard = lock_thread_event_store(&store);
            if guard.session.is_none()
                && let Some(session) = inferred_session
            {
                guard.session = Some(session);
            }
            let had_pending_approvals = guard.has_pending_thread_approvals();
            let live_notification = guard.active.then(|| notification.clone());
            let sequence = guard.push_notification(notification);
            let has_pending = guard.has_pending_thread_approvals();
            (
                live_notification,
                sequence,
                had_pending_approvals != has_pending,
                has_pending,
            )
        };

        if let Some(notification) = live_notification {
            let envelope = ThreadEventEnvelope {
                sequence,
                event: ThreadBufferedEvent::Notification(notification),
            };
            if let Err(err) = self.thread_event_channels[&thread_id].sender.send(envelope) {
                tracing::warn!("thread {thread_id} event channel closed: {err}");
            }
        }
        if pending_approval_visibility_changed {
            self.set_pending_thread_approval_state(thread_id, has_pending);
            if self.active_thread_id != Some(thread_id) {
                self.refresh_pending_thread_approvals();
            }
        }
        Ok(())
    }

    /// Eagerly fetches nickname and role for receiver threads referenced by a collab notification.
    ///
    /// This runs on every buffered thread notification before it reaches rendering. For each
    /// receiver thread id that the navigation cache does not yet have metadata for, it issues a
    /// `thread/read` RPC and registers the result in both `AgentNavigationState` and the
    /// `ChatWidget` metadata map. Threads that already have a nickname or role cached are skipped,
    /// so the cost is at most one RPC per thread over the lifetime of a session.
    ///
    /// Failures are logged and silently ignored -- the worst outcome is that a rendered item shows
    /// a thread id instead of a human-readable name, which is the same behavior the TUI had before
    /// this change.
    pub(super) async fn hydrate_collab_agent_metadata_for_notification(
        &mut self,
        app_gateway: &mut AppGatewaySession,
        notification: &ServerNotification,
    ) {
        let Some(receiver_thread_ids) = collab_receiver_thread_ids(notification) else {
            return;
        };

        for receiver_thread_id in receiver_thread_ids {
            let Ok(thread_id) = ThreadId::from_string(receiver_thread_id) else {
                tracing::warn!(
                    thread_id = receiver_thread_id,
                    "ignoring collab receiver with invalid thread id during metadata hydration"
                );
                continue;
            };

            if self.agent_navigation.get(&thread_id).is_some_and(|entry| {
                entry.agent_base_name.is_some()
                    || entry.agent_title.is_some()
                    || entry.agent_display_name.is_some()
                    || entry.agent_role.is_some()
            }) {
                continue;
            }

            match app_gateway
                .thread_read(thread_id, /*include_turns*/ false)
                .await
            {
                Ok(thread) => {
                    self.upsert_agent_picker_thread(
                        thread_id,
                        thread.agent_base_name,
                        thread.agent_title,
                        thread.agent_display_name,
                        thread.agent_role,
                        /*is_closed*/ false,
                    );
                }
                Err(err) => {
                    tracing::warn!(
                        thread_id = %thread_id,
                        error = %err,
                        "failed to hydrate collab receiver thread metadata"
                    );
                }
            }
        }
    }

    pub(super) fn infer_session_for_thread_notification(
        &mut self,
        thread_id: ThreadId,
        notification: &ServerNotification,
    ) -> Option<ThreadSessionState> {
        let ServerNotification::ThreadStarted(notification) = notification else {
            return None;
        };
        let mut session = self.primary_session_configured.clone()?;
        session.thread_id = thread_id;
        session.thread_name = notification.thread.name.clone();
        session.model_provider_id = notification.thread.model_provider.clone();
        session.cwd = notification.thread.cwd.clone();
        let rollout_path = notification.thread.path.clone();
        if let Some(model) = notification.thread.model.clone() {
            session.model = model;
        } else if rollout_path.is_some() {
            session.model.clear();
        }
        session.history_log_id = 0;
        session.history_entry_count = 0;
        session.rollout_path = rollout_path;
        session.selfwork_plan_path = notification.thread.selfwork_plan_path.clone();
        self.apply_current_permissions_to_thread_session(&mut session);
        self.upsert_agent_picker_thread(
            thread_id,
            notification.thread.agent_base_name.clone(),
            notification.thread.agent_title.clone(),
            notification.thread.agent_display_name.clone(),
            notification.thread.agent_role.clone(),
            /*is_closed*/ false,
        );
        Some(session)
    }

    pub(super) async fn enqueue_thread_request(
        &mut self,
        thread_id: ThreadId,
        request: ServerRequest,
    ) -> Result<()> {
        let inactive_interactive_request = if self.active_thread_id != Some(thread_id) {
            self.interactive_request_for_thread_request(thread_id, &request)
                .await
        } else {
            None
        };
        let store = Arc::clone(&self.ensure_thread_channel(thread_id).store);

        let (live_request, sequence, pending_approval_visibility_changed, has_pending) = {
            let mut guard = lock_thread_event_store(&store);
            let had_pending_approvals = guard.has_pending_thread_approvals();
            let live_request = guard.active.then(|| request.clone());
            let inserted = guard.push_request(request);
            let sequence = inserted.then(|| guard.last_event_sequence());
            let has_pending = guard.has_pending_thread_approvals();
            (
                live_request,
                sequence,
                had_pending_approvals != has_pending,
                has_pending,
            )
        };

        if let (Some(request), Some(sequence)) = (live_request, sequence) {
            let envelope = ThreadEventEnvelope {
                sequence,
                event: ThreadBufferedEvent::Request(request),
            };
            if let Err(err) = self.thread_event_channels[&thread_id].sender.send(envelope) {
                tracing::warn!("thread {thread_id} event channel closed: {err}");
            }
        } else if sequence.is_some()
            && let Some(request) = inactive_interactive_request
        {
            match request {
                ThreadInteractiveRequest::Approval(request) => {
                    self.chat_widget.push_approval_request(request);
                }
                ThreadInteractiveRequest::McpServerElicitation(request) => {
                    self.chat_widget
                        .push_mcp_server_elicitation_request(request);
                }
            }
        }
        if pending_approval_visibility_changed {
            self.set_pending_thread_approval_state(thread_id, has_pending);
            if self.active_thread_id != Some(thread_id) {
                self.refresh_pending_thread_approvals();
            }
        }
        Ok(())
    }

    pub(super) async fn enqueue_thread_history_entry_response(
        &mut self,
        thread_id: ThreadId,
        event: GetHistoryEntryResponseEvent,
    ) -> Result<()> {
        let store = Arc::clone(&self.ensure_thread_channel(thread_id).store);

        let (live_event, sequence) = {
            let mut guard = lock_thread_event_store(&store);
            let live_event = guard.active.then(|| event.clone());
            let sequence = guard.push_history_entry_response(event);
            (live_event, sequence)
        };

        if let Some(event) = live_event {
            let envelope = ThreadEventEnvelope {
                sequence,
                event: ThreadBufferedEvent::HistoryEntryResponse(event),
            };
            if let Err(err) = self.thread_event_channels[&thread_id].sender.send(envelope) {
                tracing::warn!("thread {thread_id} event channel closed: {err}");
            }
        }
        Ok(())
    }

    pub(super) async fn enqueue_primary_thread_session(
        &mut self,
        mut session: ThreadSessionState,
        turns: Vec<Turn>,
        status: ThreadStatus,
        control_state: Option<ThreadControlState>,
    ) -> Result<()> {
        self.apply_current_permissions_to_thread_session(&mut session);
        let thread_id = session.thread_id;
        self.primary_thread_id = Some(thread_id);
        self.primary_session_configured = Some(session.clone());
        self.upsert_agent_picker_thread(
            thread_id, /*agent_base_name*/ None, /*agent_title*/ None,
            /*agent_display_name*/ None, /*agent_role*/ None, /*is_closed*/ false,
        );
        let channel = self.ensure_thread_channel(thread_id);
        {
            let mut store = lock_thread_event_store(&channel.store);
            store.set_runtime_snapshot(
                session.clone(),
                turns.clone(),
                status.clone(),
                control_state.clone(),
            );
        }
        self.activate_thread_channel(thread_id);
        self.chat_widget
            .set_initial_user_message_submit_suppressed(/*suppressed*/ true);
        self.chat_widget.handle_thread_session(session);
        let turns = compact_visible_replay_turns(turns);
        self.chat_widget
            .replay_thread_turns(turns, ReplayKind::ResumeInitialMessages);
        self.chat_widget.apply_thread_status_snapshot(Some(&status));
        self.chat_widget
            .set_thread_control_state(control_state.as_ref());
        let pending = std::mem::take(&mut self.pending_primary_events);
        for pending_event in pending {
            match pending_event {
                ThreadBufferedEvent::Notification(notification) => {
                    self.enqueue_thread_notification(thread_id, notification)
                        .await?;
                }
                ThreadBufferedEvent::Request(request) => {
                    self.enqueue_thread_request(thread_id, request).await?;
                }
                ThreadBufferedEvent::HistoryEntryResponse(event) => {
                    self.enqueue_thread_history_entry_response(thread_id, event)
                        .await?;
                }
                ThreadBufferedEvent::FeedbackSubmission(event) => {
                    self.enqueue_thread_feedback_event(thread_id, event).await;
                }
            }
        }
        self.chat_widget
            .set_initial_user_message_submit_suppressed(/*suppressed*/ false);
        self.chat_widget.submit_initial_user_message_if_pending();
        Ok(())
    }

    pub(super) async fn enqueue_primary_thread_notification(
        &mut self,
        notification: ServerNotification,
    ) -> Result<()> {
        if let Some(thread_id) = self.primary_thread_id {
            return self
                .enqueue_thread_notification(thread_id, notification)
                .await;
        }
        self.pending_primary_events
            .push_back(ThreadBufferedEvent::Notification(notification));
        Ok(())
    }

    pub(super) async fn enqueue_primary_thread_request(
        &mut self,
        request: ServerRequest,
    ) -> Result<()> {
        if let Some(thread_id) = self.primary_thread_id {
            return self.enqueue_thread_request(thread_id, request).await;
        }
        self.pending_primary_events
            .push_back(ThreadBufferedEvent::Request(request));
        Ok(())
    }
}
