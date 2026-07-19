use std::any::Any;
use std::sync::Arc;

use async_trait::async_trait;
use tokio_util::sync::CancellationToken;

use crate::outcome::LoopResult;

use super::effects::EffectJournal;
use super::effects::ToolEffects;
use super::types::ToolCall;
use super::types::ToolProgress;
use super::types::ToolResult;
use super::types::ToolSpec;

#[async_trait]
pub trait ToolLifecycleSink: Send + Sync {
    async fn tool_started(&self, call: &ToolCall) -> LoopResult<()>;

    async fn tool_progress(&self, progress: ToolProgress) -> LoopResult<()>;
}

#[derive(Clone, Debug)]
pub struct ToolExecutionContext {
    pub cancel: CancellationToken,
    pub effects: EffectJournal,
}

pub struct PreparedToolCall {
    effects: ToolEffects,
    state: Option<Box<dyn Any + Send>>,
}

impl PreparedToolCall {
    pub fn new(effects: ToolEffects) -> Self {
        Self {
            effects,
            state: None,
        }
    }

    pub fn with_state<T>(mut self, state: T) -> Self
    where
        T: Any + Send,
    {
        self.state = Some(Box::new(state));
        self
    }

    pub fn effects(&self) -> &ToolEffects {
        &self.effects
    }

    pub fn take_state<T>(&mut self) -> Option<T>
    where
        T: Any + Send,
    {
        self.state.take()?.downcast::<T>().ok().map(|state| *state)
    }
}

impl ToolExecutionContext {
    pub fn new(cancel: CancellationToken, effects: EffectJournal) -> Self {
        Self { cancel, effects }
    }
}

#[async_trait]
pub trait Tool: Send + Sync {
    fn spec(&self) -> ToolSpec;

    async fn prepare(&self, _call: &ToolCall) -> LoopResult<PreparedToolCall> {
        Ok(PreparedToolCall::new(ToolEffects::unknown_write()))
    }

    async fn execute(
        &self,
        call: ToolCall,
        context: ToolExecutionContext,
    ) -> LoopResult<ToolResult>;

    async fn execute_streaming(
        &self,
        call: ToolCall,
        context: ToolExecutionContext,
        _lifecycle: &(dyn ToolLifecycleSink + Send + Sync),
    ) -> LoopResult<ToolResult> {
        self.execute(call, context).await
    }

    async fn execute_prepared_streaming(
        &self,
        call: ToolCall,
        _prepared: PreparedToolCall,
        context: ToolExecutionContext,
        lifecycle: &(dyn ToolLifecycleSink + Send + Sync),
    ) -> LoopResult<ToolResult> {
        self.execute_streaming(call, context, lifecycle).await
    }
}

pub trait ToolRegistry: Send + Sync {
    fn get(&self, name: &str) -> Option<Arc<dyn Tool>>;
}
