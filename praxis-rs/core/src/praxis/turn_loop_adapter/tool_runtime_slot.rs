use std::sync::Arc;
use std::sync::RwLock;

use praxis_loop::outcome::LoopResult;
use praxis_loop::outcome::TurnError;
use praxis_loop::outcome::TurnErrorKind;

use crate::tools::tool_call_runtime::ToolCallRuntime;

#[derive(Clone, Default)]
pub(super) struct ModelRoundToolsSlot {
    inner: Arc<RwLock<Option<ToolCallRuntime>>>,
}

impl ModelRoundToolsSlot {
    pub(super) fn store(&self, runtime: ToolCallRuntime) -> LoopResult<()> {
        let mut guard = self.inner.write().map_err(|_| lock_error())?;
        *guard = Some(runtime);
        Ok(())
    }

    pub(super) fn current(&self) -> Option<ToolCallRuntime> {
        self.inner.read().ok()?.as_ref().cloned()
    }
}

fn lock_error() -> TurnError {
    TurnError::new(
        TurnErrorKind::Internal,
        "round tool runtime state lock was poisoned",
    )
}
