use crate::praxis::Session;
use crate::praxis::TurnContext;
use praxis_protocol::protocol::EventMsg;
use praxis_protocol::protocol::UndoCompletedEvent;
use praxis_protocol::protocol::UndoStartedEvent;

pub(super) async fn send_undo_started(session: &Session, ctx: &TurnContext) {
    session
        .send_event(
            ctx,
            EventMsg::UndoStarted(UndoStartedEvent {
                message: Some("Undo in progress...".to_string()),
            }),
        )
        .await;
}

pub(super) async fn send_undo_completed(
    session: &Session,
    ctx: &TurnContext,
    event: UndoCompletedEvent,
) {
    session
        .send_event(ctx, EventMsg::UndoCompleted(event))
        .await;
}

pub(super) fn undo_completed(
    success: bool,
    message: impl Into<Option<String>>,
) -> UndoCompletedEvent {
    UndoCompletedEvent {
        success,
        message: message.into(),
    }
}
