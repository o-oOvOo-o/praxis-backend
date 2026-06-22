use std::sync::Arc;

use async_trait::async_trait;
use tokio_util::sync::CancellationToken;

use crate::outcome::LoopResult;

use super::types::ConcurrencyMode;
use super::types::ToolCall;
use super::types::ToolProgress;
use super::types::ToolResult;
use super::types::ToolSpec;

#[async_trait]
pub trait ToolLifecycleSink: Send + Sync {
    async fn tool_started(&self, call: &ToolCall) -> LoopResult<()>;

    async fn tool_progress(&self, progress: ToolProgress) -> LoopResult<()>;
}

#[async_trait]
pub trait Tool: Send + Sync {
    fn spec(&self) -> ToolSpec;

    fn concurrency(&self) -> ConcurrencyMode {
        self.spec().concurrency
    }

    async fn execute(&self, call: ToolCall, cancel: CancellationToken) -> LoopResult<ToolResult>;

    async fn execute_streaming(
        &self,
        call: ToolCall,
        cancel: CancellationToken,
        _lifecycle: &(dyn ToolLifecycleSink + Send + Sync),
    ) -> LoopResult<ToolResult> {
        self.execute(call, cancel).await
    }
}

pub trait ToolRegistry: Send + Sync {
    fn get(&self, name: &str) -> Option<Arc<dyn Tool>>;
}
