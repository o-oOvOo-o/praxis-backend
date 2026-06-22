use async_trait::async_trait;

use crate::model::TurnEvent;
use crate::outcome::LoopResult;
use crate::tool::ToolCall;
use crate::tool::ToolLifecycleSink;
use crate::tool::ToolProgress;

#[async_trait]
pub trait EventSink: Send + Sync {
    async fn emit_event(&self, event: TurnEvent) -> LoopResult<()>;
}

#[async_trait]
impl<T: ?Sized> ToolLifecycleSink for T
where
    T: EventSink + Send + Sync,
{
    async fn tool_started(&self, call: &ToolCall) -> LoopResult<()> {
        self.emit_event(TurnEvent::ToolStarted {
            call_id: call.id.clone(),
            name: call.name.clone(),
        })
        .await
    }

    async fn tool_progress(&self, progress: ToolProgress) -> LoopResult<()> {
        self.emit_event(TurnEvent::ToolProgress {
            call_id: progress.call_id,
            content: progress.content,
        })
        .await
    }
}
