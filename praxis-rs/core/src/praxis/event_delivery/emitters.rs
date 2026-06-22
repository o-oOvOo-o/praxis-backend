use praxis_protocol::protocol::ErrorEvent;
use praxis_protocol::protocol::Event;
use praxis_protocol::protocol::EventMsg;
use praxis_protocol::protocol::PraxisErrorInfo;
use praxis_protocol::protocol::WarningEvent;

use crate::praxis::Session;
use crate::praxis::TurnContext;

use super::make_error_event;
use super::make_warning_event;

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
        praxis_error_info: Option<PraxisErrorInfo>,
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
        praxis_error_info: Option<PraxisErrorInfo>,
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
}
