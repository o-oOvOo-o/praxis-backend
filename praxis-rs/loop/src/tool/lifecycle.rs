use async_trait::async_trait;
use tokio::sync::mpsc;
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::sync::mpsc::UnboundedSender;

use crate::model::TurnItem;
use crate::outcome::LoopResult;
use crate::outcome::TurnError;
use crate::outcome::TurnErrorKind;
use crate::tool::ToolCall;
use crate::tool::ToolLifecycleSink;
use crate::tool::ToolProgress;

pub(super) struct RecordedToolLifecycle<'a, P: ToolLifecycleSink + ?Sized> {
    inner: &'a P,
    items: UnboundedSender<TurnItem>,
}

pub(super) struct RecordedToolLifecycleDrain {
    items: UnboundedReceiver<TurnItem>,
}

impl<'a, P> RecordedToolLifecycle<'a, P>
where
    P: ToolLifecycleSink + ?Sized,
{
    pub(super) fn new(inner: &'a P) -> (Self, RecordedToolLifecycleDrain) {
        let (items_tx, items_rx) = mpsc::unbounded_channel();
        (
            Self {
                inner,
                items: items_tx,
            },
            RecordedToolLifecycleDrain { items: items_rx },
        )
    }

    fn record_item(&self, item: TurnItem) -> LoopResult<()> {
        self.items.send(item).map_err(|_| {
            TurnError::new(
                TurnErrorKind::Internal,
                "tool lifecycle recorder receiver was closed",
            )
        })
    }
}

impl RecordedToolLifecycleDrain {
    pub(super) fn finish(mut self) -> Vec<TurnItem> {
        self.items.close();
        let mut items = Vec::new();
        while let Ok(item) = self.items.try_recv() {
            items.push(item);
        }
        items
    }
}

#[async_trait]
impl<P> ToolLifecycleSink for RecordedToolLifecycle<'_, P>
where
    P: ToolLifecycleSink + ?Sized,
{
    async fn tool_started(&self, call: &ToolCall) -> LoopResult<()> {
        self.record_item(TurnItem::ToolStarted {
            call_id: call.id.clone(),
            name: call.name.clone(),
        })?;
        self.inner.tool_started(call).await
    }

    async fn tool_progress(&self, progress: ToolProgress) -> LoopResult<()> {
        self.record_item(TurnItem::ToolProgress {
            call_id: progress.call_id.clone(),
            content: progress.content.clone(),
        })?;
        self.inner.tool_progress(progress).await
    }
}
