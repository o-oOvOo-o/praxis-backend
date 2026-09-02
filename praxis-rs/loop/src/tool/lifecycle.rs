use async_trait::async_trait;
use std::sync::Arc;
use std::sync::Mutex;

use crate::model::TurnItem;
use crate::outcome::LoopResult;
use crate::outcome::TurnError;
use crate::outcome::TurnErrorKind;
use crate::tool::ToolCall;
use crate::tool::ToolLifecycleSink;
use crate::tool::ToolProgress;
use crate::tool::ToolResult;

pub(super) struct RecordedToolLifecycle<'a, P: ToolLifecycleSink + ?Sized> {
    inner: &'a P,
    items: Arc<Mutex<Vec<TurnItem>>>,
}

pub(super) struct RecordedToolLifecycleDrain {
    items: Arc<Mutex<Vec<TurnItem>>>,
}

impl<'a, P> RecordedToolLifecycle<'a, P>
where
    P: ToolLifecycleSink + ?Sized,
{
    pub(super) fn new(inner: &'a P) -> (Self, RecordedToolLifecycleDrain) {
        let items = Arc::new(Mutex::new(Vec::new()));
        (
            Self {
                inner,
                items: Arc::clone(&items),
            },
            RecordedToolLifecycleDrain { items },
        )
    }

    fn record_item(&self, item: TurnItem) -> LoopResult<()> {
        self.items
            .lock()
            .map_err(|_| {
                TurnError::new(
                    TurnErrorKind::Internal,
                    "tool lifecycle recorder lock was poisoned",
                )
            })?
            .push(item);
        Ok(())
    }
}

impl RecordedToolLifecycleDrain {
    pub(super) fn finish(self) -> Vec<TurnItem> {
        match Arc::try_unwrap(self.items) {
            Ok(items) => items
                .into_inner()
                .unwrap_or_else(|error| error.into_inner()),
            Err(items) => {
                let mut items = items.lock().unwrap_or_else(|error| error.into_inner());
                std::mem::take(&mut *items)
            }
        }
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

    async fn tool_execution_completed(&self, result: &ToolResult) -> LoopResult<()> {
        self.inner.tool_execution_completed(result).await
    }
}
