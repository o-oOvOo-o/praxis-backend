use praxis_protocol::protocol::PraxisErrorInfo;

use crate::praxis::Session;

pub(super) async fn reject_invalid_request(
    session: &Session,
    event_id: &str,
    num_turns: u32,
) -> bool {
    if num_turns == 0 {
        session
            .raw_event_emitter(event_id)
            .error(
                "num_turns must be >= 1",
                Some(PraxisErrorInfo::ThreadRollbackFailed),
            )
            .await;
        return true;
    }

    let has_active_turn = { session.active_turn.lock().await.is_some() };
    if has_active_turn {
        session
            .raw_event_emitter(event_id)
            .error(
                "Cannot rollback while a turn is in progress.",
                Some(PraxisErrorInfo::ThreadRollbackFailed),
            )
            .await;
        return true;
    }

    false
}
