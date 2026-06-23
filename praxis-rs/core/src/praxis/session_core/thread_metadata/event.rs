use praxis_protocol::protocol::Event;
use praxis_protocol::protocol::EventMsg;
use praxis_protocol::protocol::ThreadNameUpdatedEvent;

use crate::praxis::Session;

pub(super) async fn emit_thread_name_updated(session: &Session, event_id: String, name: String) {
    session
        .send_event_raw(Event {
            id: event_id,
            msg: EventMsg::ThreadNameUpdated(ThreadNameUpdatedEvent {
                thread_id: session.conversation_id,
                thread_name: Some(name),
            }),
        })
        .await;
}
