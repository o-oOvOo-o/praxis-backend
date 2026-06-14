use super::*;

pub(crate) fn make_warning_event(id: impl Into<String>, message: impl Into<String>) -> Event {
    Event {
        id: id.into(),
        msg: EventMsg::Warning(WarningEvent {
            message: message.into(),
        }),
    }
}

pub(crate) fn make_error_event(
    id: impl Into<String>,
    message: impl Into<String>,
    praxis_error_info: Option<CodexErrorInfo>,
) -> Event {
    Event {
        id: id.into(),
        msg: EventMsg::Error(ErrorEvent {
            message: message.into(),
            praxis_error_info,
        }),
    }
}

pub(crate) fn make_deprecation_notice_event(
    id: impl Into<String>,
    summary: impl Into<String>,
    details: Option<String>,
) -> Event {
    Event {
        id: id.into(),
        msg: EventMsg::DeprecationNotice(DeprecationNoticeEvent {
            summary: summary.into(),
            details,
        }),
    }
}

pub(crate) struct SessionEventEmitter<'a> {
    session: &'a Session,
    event_id: String,
}

pub(crate) struct TurnEventEmitter<'session, 'turn> {
    session: &'session Session,
    turn_context: &'turn TurnContext,
}

impl<'a> SessionEventEmitter<'a> {
    pub(crate) fn new(session: &'a Session, event_id: impl Into<String>) -> Self {
        Self {
            session,
            event_id: event_id.into(),
        }
    }

    pub(crate) async fn warning(&self, message: impl Into<String>) {
        self.session
            .send_event_raw(make_warning_event(self.event_id.clone(), message))
            .await;
    }

    pub(crate) async fn error(
        &self,
        message: impl Into<String>,
        praxis_error_info: Option<CodexErrorInfo>,
    ) {
        self.session
            .send_event_raw(make_error_event(
                self.event_id.clone(),
                message,
                praxis_error_info,
            ))
            .await;
    }

    pub(crate) async fn error_event(&self, event: ErrorEvent) {
        self.session
            .send_event_raw(Event {
                id: self.event_id.clone(),
                msg: EventMsg::Error(event),
            })
            .await;
    }
}

impl<'session, 'turn> TurnEventEmitter<'session, 'turn> {
    pub(crate) fn new(session: &'session Session, turn_context: &'turn TurnContext) -> Self {
        Self {
            session,
            turn_context,
        }
    }

    pub(crate) async fn warning(&self, message: impl Into<String>) {
        self.session
            .send_event(
                self.turn_context,
                EventMsg::Warning(WarningEvent {
                    message: message.into(),
                }),
            )
            .await;
    }

    pub(crate) async fn error(
        &self,
        message: impl Into<String>,
        praxis_error_info: Option<CodexErrorInfo>,
    ) {
        self.session
            .send_event(
                self.turn_context,
                EventMsg::Error(ErrorEvent {
                    message: message.into(),
                    praxis_error_info,
                }),
            )
            .await;
    }

    pub(crate) async fn error_event(&self, event: ErrorEvent) {
        self.session
            .send_event(self.turn_context, EventMsg::Error(event))
            .await;
    }
}

impl Session {
    pub(crate) fn raw_event_emitter(&self, event_id: impl Into<String>) -> SessionEventEmitter<'_> {
        SessionEventEmitter::new(self, event_id)
    }

    pub(crate) fn turn_event_emitter<'turn>(
        &self,
        turn_context: &'turn TurnContext,
    ) -> TurnEventEmitter<'_, 'turn> {
        TurnEventEmitter::new(self, turn_context)
    }

    /// Persist the event to rollout and send it to clients.
    pub(crate) async fn send_event(&self, turn_context: &TurnContext, msg: EventMsg) {
        let legacy_source = msg.clone();
        let event = Event {
            id: turn_context.sub_id.clone(),
            msg,
        };
        self.send_event_raw(event).await;
        self.maybe_notify_parent_of_terminal_turn(turn_context, &legacy_source)
            .await;
        self.maybe_mirror_event_text_to_realtime(&legacy_source)
            .await;
        self.maybe_clear_realtime_handoff_for_event(&legacy_source)
            .await;

        let show_raw_agent_reasoning = self.show_raw_agent_reasoning();
        for legacy in legacy_source.as_legacy_events(show_raw_agent_reasoning) {
            let legacy_event = Event {
                id: turn_context.sub_id.clone(),
                msg: legacy,
            };
            self.send_event_raw(legacy_event).await;
        }
    }

    /// Forwards terminal turn events from spawned children to their direct parent.
    pub(super) async fn maybe_notify_parent_of_terminal_turn(
        &self,
        turn_context: &TurnContext,
        msg: &EventMsg,
    ) {
        if !matches!(msg, EventMsg::TurnComplete(_) | EventMsg::TurnAborted(_)) {
            return;
        }

        let SessionSource::SubAgent(SubAgentSource::ThreadSpawn {
            parent_thread_id,
            agent_path: Some(child_agent_path),
            ..
        }) = &turn_context.session_source
        else {
            return;
        };

        let Some(status) = agent_status_from_event(msg) else {
            return;
        };
        if !is_final(&status) {
            return;
        }

        self.forward_child_completion_to_parent(*parent_thread_id, child_agent_path, status)
            .await;
    }

    /// Sends the standard completion envelope from a spawned child to its parent.
    pub(super) async fn forward_child_completion_to_parent(
        &self,
        parent_thread_id: ThreadId,
        child_agent_path: &praxis_protocol::AgentPath,
        status: AgentStatus,
    ) {
        let Some(parent_agent_path) = child_agent_path
            .as_str()
            .rsplit_once('/')
            .and_then(|(parent, _)| praxis_protocol::AgentPath::try_from(parent).ok())
        else {
            return;
        };

        let message = format_subagent_notification_message(child_agent_path.as_str(), &status);
        let communication = InterAgentCommunication::new(
            child_agent_path.clone(),
            parent_agent_path,
            Vec::new(),
            message,
            /*trigger_turn*/ false,
        );
        if let Err(err) = self
            .services
            .agent_control
            .send_inter_agent_communication(parent_thread_id, communication)
            .await
        {
            debug!("failed to notify parent thread {parent_thread_id}: {err}");
        }
    }

    pub(super) async fn maybe_mirror_event_text_to_realtime(&self, msg: &EventMsg) {
        let Some(text) = realtime_text_for_event(msg) else {
            return;
        };
        if self.conversation.running_state().await.is_none()
            || self.conversation.active_handoff_id().await.is_none()
        {
            return;
        }
        if let Err(err) = self.conversation.handoff_out(text).await {
            debug!("failed to mirror event text to realtime conversation: {err}");
        }
    }

    pub(super) async fn maybe_clear_realtime_handoff_for_event(&self, msg: &EventMsg) {
        if !matches!(msg, EventMsg::TurnComplete(_)) {
            return;
        }
        if let Err(err) = self.conversation.handoff_complete().await {
            debug!("failed to finalize realtime handoff output: {err}");
        }
        self.conversation.clear_active_handoff().await;
    }

    pub(crate) async fn send_event_raw(&self, event: Event) {
        // Persist the event into rollout (recorder filters as needed)
        let rollout_items = vec![RolloutItem::EventMsg(event.msg.clone())];
        self.persist_rollout_items(&rollout_items).await;
        self.deliver_event_raw(event).await;
    }

    pub(super) async fn deliver_event_raw(&self, event: Event) {
        // Record the last known agent status.
        if let Some(status) = agent_status_from_event(&event.msg) {
            self.agent_status.send_replace(status);
        }
        if let Err(e) = self.tx_event.send(event).await {
            debug!("dropping event because channel is closed: {e}");
        }
    }

    pub(crate) async fn emit_turn_item_started(&self, turn_context: &TurnContext, item: &TurnItem) {
        self.send_event(
            turn_context,
            EventMsg::ItemStarted(ItemStartedEvent {
                thread_id: self.conversation_id,
                turn_id: turn_context.sub_id.clone(),
                item: item.clone(),
            }),
        )
        .await;
    }

    pub(crate) async fn emit_turn_item_completed(
        &self,
        turn_context: &TurnContext,
        item: TurnItem,
    ) {
        record_turn_ttfm_metric(turn_context, &item).await;
        self.send_event(
            turn_context,
            EventMsg::ItemCompleted(ItemCompletedEvent {
                thread_id: self.conversation_id,
                turn_id: turn_context.sub_id.clone(),
                item,
            }),
        )
        .await;
    }
}
