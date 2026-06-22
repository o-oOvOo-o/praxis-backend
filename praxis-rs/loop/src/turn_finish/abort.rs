use crate::model::TurnEvent;
use crate::outcome::TurnError;
use crate::outcome::TurnResult;
use crate::services::TurnServices;
use crate::state::TurnState;

pub(crate) async fn abort_with_event<S>(
    state: TurnState,
    services: &S,
    reason: TurnError,
) -> TurnResult
where
    S: TurnServices + ?Sized,
{
    let _ = services
        .emit_event(TurnEvent::TurnAborted(reason.message.clone()))
        .await;
    TurnResult::Aborted { state, reason }
}
