use crate::model::TurnEvent;
use crate::model::TurnItem;
use crate::outcome::LoopResult;
use crate::services::EventSink;
use crate::services::HistorySink;
use crate::state::TurnState;

pub(crate) async fn persist_turn_items<S>(
    items: &[TurnItem],
    state: &mut TurnState,
    services: &S,
) -> LoopResult<()>
where
    S: EventSink + HistorySink + ?Sized,
{
    if items.is_empty() {
        return Ok(());
    }

    services.persist_items(items).await?;
    state.record_items(items.to_vec());

    for item in items {
        if let TurnItem::ToolResult(result) = item {
            services
                .emit_event(TurnEvent::ToolFinished(result.clone()))
                .await?;
        }
    }
    Ok(())
}
