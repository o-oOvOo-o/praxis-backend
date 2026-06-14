use super::thread_store_api::ThreadHistorySource;
use super::thread_store_api::hydrate_thread_turns;
use super::*;
use praxis_app_gateway_protocol::ThreadControlState;

#[derive(Clone)]
pub(crate) struct ListenerTaskContext {
    pub(crate) thread_manager: Arc<ThreadManager>,
    pub(crate) thread_state_manager: ThreadStateManager,
    pub(crate) outgoing: Arc<OutgoingMessageSender>,
    pub(crate) analytics_events_client: AnalyticsEventsClient,
    pub(crate) general_analytics_enabled: bool,
    pub(crate) thread_watch_manager: ThreadWatchManager,
    pub(crate) fallback_model_provider: String,
    pub(crate) praxis_home: PathBuf,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum EnsureConversationListenerResult {
    Attached,
    ConnectionClosed,
}

impl PraxisMessageProcessor {
    pub(super) async fn ensure_conversation_listener(
        &self,
        conversation_id: ThreadId,
        connection_id: ConnectionId,
        raw_events_enabled: bool,
    ) -> Result<EnsureConversationListenerResult, JSONRPCErrorError> {
        Self::ensure_conversation_listener_task(
            ListenerTaskContext {
                thread_manager: Arc::clone(&self.thread_manager),
                thread_state_manager: self.thread_state_manager.clone(),
                outgoing: Arc::clone(&self.outgoing),
                analytics_events_client: self.analytics_events_client.clone(),
                general_analytics_enabled: self.config.features.enabled(Feature::GeneralAnalytics),
                thread_watch_manager: self.thread_watch_manager.clone(),
                fallback_model_provider: self.config.model_provider_id.clone(),
                praxis_home: self.config.praxis_home.clone(),
            },
            conversation_id,
            connection_id,
            raw_events_enabled,
        )
        .await
    }

    pub(super) async fn ensure_conversation_listener_task(
        listener_task_context: ListenerTaskContext,
        conversation_id: ThreadId,
        connection_id: ConnectionId,
        raw_events_enabled: bool,
    ) -> Result<EnsureConversationListenerResult, JSONRPCErrorError> {
        let conversation = match listener_task_context
            .thread_manager
            .get_thread(conversation_id)
            .await
        {
            Ok(conv) => conv,
            Err(_) => {
                return Err(JSONRPCErrorError {
                    code: INVALID_REQUEST_ERROR_CODE,
                    message: format!("thread not found: {conversation_id}"),
                    data: None,
                });
            }
        };
        let Some(thread_state) = listener_task_context
            .thread_state_manager
            .try_ensure_connection_subscribed(conversation_id, connection_id, raw_events_enabled)
            .await
        else {
            return Ok(EnsureConversationListenerResult::ConnectionClosed);
        };
        Self::ensure_listener_task_running_task(
            listener_task_context,
            conversation_id,
            conversation,
            thread_state,
        )
        .await;
        Ok(EnsureConversationListenerResult::Attached)
    }

    pub(super) fn log_listener_attach_result(
        result: Result<EnsureConversationListenerResult, JSONRPCErrorError>,
        thread_id: ThreadId,
        connection_id: ConnectionId,
        thread_kind: &'static str,
    ) {
        match result {
            Ok(EnsureConversationListenerResult::Attached) => {}
            Ok(EnsureConversationListenerResult::ConnectionClosed) => {
                tracing::debug!(
                    thread_id = %thread_id,
                    connection_id = ?connection_id,
                    "skipping auto-attach for closed connection"
                );
            }
            Err(err) => {
                tracing::warn!(
                    "failed to attach listener for {thread_kind} {thread_id}: {message}",
                    message = err.message
                );
            }
        }
    }

    pub(super) async fn ensure_listener_task_running(
        &self,
        conversation_id: ThreadId,
        conversation: Arc<PraxisThread>,
        thread_state: Arc<Mutex<ThreadState>>,
    ) {
        Self::ensure_listener_task_running_task(
            ListenerTaskContext {
                thread_manager: Arc::clone(&self.thread_manager),
                thread_state_manager: self.thread_state_manager.clone(),
                outgoing: Arc::clone(&self.outgoing),
                analytics_events_client: self.analytics_events_client.clone(),
                general_analytics_enabled: self.config.features.enabled(Feature::GeneralAnalytics),
                thread_watch_manager: self.thread_watch_manager.clone(),
                fallback_model_provider: self.config.model_provider_id.clone(),
                praxis_home: self.config.praxis_home.clone(),
            },
            conversation_id,
            conversation,
            thread_state,
        )
        .await;
    }

    pub(super) async fn ensure_listener_task_running_task(
        listener_task_context: ListenerTaskContext,
        conversation_id: ThreadId,
        conversation: Arc<PraxisThread>,
        thread_state: Arc<Mutex<ThreadState>>,
    ) {
        let (cancel_tx, mut cancel_rx) = oneshot::channel();
        let (mut listener_command_rx, listener_generation) = {
            let mut thread_state = thread_state.lock().await;
            if thread_state.listener_matches(&conversation) {
                return;
            }
            thread_state.set_listener(cancel_tx, &conversation)
        };
        let ListenerTaskContext {
            outgoing,
            thread_manager,
            thread_state_manager,
            analytics_events_client: _,
            general_analytics_enabled: _,
            thread_watch_manager,
            fallback_model_provider,
            praxis_home,
        } = listener_task_context;
        let outgoing_for_task = Arc::clone(&outgoing);
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = &mut cancel_rx => {
                        // Listener was superseded or the thread is being torn down.
                        break;
                    }
                    event = conversation.next_event() => {
                        let event = match event {
                            Ok(event) => event,
                            Err(err) => {
                                tracing::warn!("thread.next_event() failed with: {err}");
                                break;
                            }
                        };

                        // Track the event before emitting any typed
                        // translations so thread-local state such as raw event
                        // opt-in stays synchronized with the conversation.
                        let raw_events_enabled = {
                            let mut thread_state = thread_state.lock().await;
                            thread_state.track_current_turn_event(&event.msg);
                            thread_state.experimental_raw_events
                        };
                        let subscribed_connection_ids = thread_state_manager
                            .subscribed_connection_ids(conversation_id)
                            .await;
                        if let EventMsg::RawResponseItem(_) = &event.msg && !raw_events_enabled {
                            continue;
                        }

                        let thread_outgoing = ThreadScopedOutgoingMessageSender::new(
                            outgoing_for_task.clone(),
                            subscribed_connection_ids,
                            conversation_id,
                        );
                        apply_bespoke_event_handling(
                            event.clone(),
                            conversation_id,
                            conversation.clone(),
                            thread_manager.clone(),
                            thread_outgoing,
                            thread_state.clone(),
                            thread_watch_manager.clone(),
                            fallback_model_provider.clone(),
                            praxis_home.as_path(),
                        )
                        .await;
                    }
                    listener_command = listener_command_rx.recv() => {
                        let Some(listener_command) = listener_command else {
                            break;
                        };
                        handle_thread_listener_command(
                            conversation_id,
                            &conversation,
                            praxis_home.as_path(),
                            &thread_state_manager,
                            &thread_state,
                            &thread_watch_manager,
                            &outgoing_for_task,
                            listener_command,
                        )
                        .await;
                    }
                }
            }

            let mut thread_state = thread_state.lock().await;
            if thread_state.listener_generation == listener_generation {
                thread_state.clear_listener();
            }
        });
    }
}

#[allow(clippy::too_many_arguments)]
async fn handle_thread_listener_command(
    conversation_id: ThreadId,
    conversation: &Arc<PraxisThread>,
    praxis_home: &Path,
    thread_state_manager: &ThreadStateManager,
    thread_state: &Arc<Mutex<ThreadState>>,
    thread_watch_manager: &ThreadWatchManager,
    outgoing: &Arc<OutgoingMessageSender>,
    listener_command: ThreadListenerCommand,
) {
    match listener_command {
        ThreadListenerCommand::SendThreadResumeResponse(resume_request) => {
            handle_pending_thread_resume_request(
                conversation_id,
                conversation,
                praxis_home,
                thread_state_manager,
                thread_state,
                thread_watch_manager,
                outgoing,
                *resume_request,
            )
            .await;
        }
        ThreadListenerCommand::ResolveServerRequest {
            request_id,
            completion_tx,
        } => {
            resolve_pending_server_request(
                conversation_id,
                thread_state_manager,
                outgoing,
                request_id,
            )
            .await;
            let _ = completion_tx.send(());
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn handle_pending_thread_resume_request(
    conversation_id: ThreadId,
    conversation: &Arc<PraxisThread>,
    praxis_home: &Path,
    thread_state_manager: &ThreadStateManager,
    thread_state: &Arc<Mutex<ThreadState>>,
    thread_watch_manager: &ThreadWatchManager,
    outgoing: &Arc<OutgoingMessageSender>,
    pending: crate::thread_state::PendingThreadResumeRequest,
) {
    let active_turn = {
        let state = thread_state.lock().await;
        state.active_turn_snapshot()
    };
    tracing::debug!(
        thread_id = %conversation_id,
        request_id = ?pending.request_id,
        active_turn_present = active_turn.is_some(),
        active_turn_id = ?active_turn.as_ref().map(|turn| turn.id.as_str()),
        active_turn_status = ?active_turn.as_ref().map(|turn| &turn.status),
        "composing running thread resume response"
    );
    let has_live_in_progress_turn =
        matches!(conversation.agent_status().await, AgentStatus::Running)
            || active_turn
                .as_ref()
                .is_some_and(|turn| matches!(turn.status, TurnStatus::InProgress));

    let request_id = pending.request_id;
    let connection_id = request_id.connection_id;
    let mut thread = pending.thread_summary;
    if let Err(message) = hydrate_thread_turns(
        &mut thread,
        ThreadHistorySource::RolloutPath(pending.rollout_path.as_path()),
        active_turn.as_ref(),
    )
    .await
    {
        outgoing
            .send_error(
                request_id,
                JSONRPCErrorError {
                    code: INTERNAL_ERROR_CODE,
                    message,
                    data: None,
                },
            )
            .await;
        return;
    }

    let thread_status = thread_watch_manager
        .loaded_status_for_thread(&thread.id)
        .await;

    let control_state = thread_watch_manager
        .loaded_control_state_for_thread(&thread.id)
        .await;
    set_thread_status_and_interrupt_stale_turns(
        &mut thread,
        thread_status,
        has_live_in_progress_turn,
        control_state.as_ref(),
    );
    thread.control_state = control_state;

    let state_db = praxis_rollout::state_db::open_if_present(praxis_home, "").await;
    thread.name = praxis_rollout::ThreadNameResolver::new(state_db.as_deref())
        .resolve_name(conversation_id)
        .await;

    let ThreadConfigSnapshot {
        model,
        model_provider_id,
        service_tier,
        approval_policy,
        approvals_reviewer,
        sandbox_policy,
        cwd,
        reasoning_effort,
        ..
    } = pending.config_snapshot;
    let response = ThreadResumeResponse {
        thread,
        model,
        model_provider: model_provider_id,
        service_tier,
        cwd,
        approval_policy: approval_policy.into(),
        approvals_reviewer: approvals_reviewer.into(),
        sandbox: sandbox_policy.into(),
        reasoning_effort,
    };
    outgoing.send_response(request_id, response).await;
    outgoing
        .replay_requests_to_connection_for_thread(connection_id, conversation_id)
        .await;
    let _attached = thread_state_manager
        .try_add_connection_to_thread(conversation_id, connection_id)
        .await;
}

async fn resolve_pending_server_request(
    conversation_id: ThreadId,
    thread_state_manager: &ThreadStateManager,
    outgoing: &Arc<OutgoingMessageSender>,
    request_id: RequestId,
) {
    let thread_id = conversation_id.to_string();
    let subscribed_connection_ids = thread_state_manager
        .subscribed_connection_ids(conversation_id)
        .await;
    let outgoing = ThreadScopedOutgoingMessageSender::new(
        outgoing.clone(),
        subscribed_connection_ids,
        conversation_id,
    );
    outgoing
        .send_server_notification(ServerNotification::ServerRequestResolved(
            ServerRequestResolvedNotification {
                thread_id,
                request_id,
            },
        ))
        .await;
}

pub(super) fn set_thread_status_and_interrupt_stale_turns(
    thread: &mut Thread,
    loaded_status: ThreadStatus,
    has_live_in_progress_turn: bool,
    control_state: Option<&ThreadControlState>,
) {
    let status = resolve_thread_status(loaded_status, has_live_in_progress_turn, control_state);
    if !matches!(status, ThreadStatus::Active { .. }) {
        for turn in &mut thread.turns {
            if matches!(turn.status, TurnStatus::InProgress) {
                turn.status = TurnStatus::Interrupted;
            }
        }
    }
    thread.status = status;
}
