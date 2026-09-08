use async_trait::async_trait;
use praxis_loop::outcome::LoopResult;
use praxis_loop::outcome::TurnError;
use praxis_loop::outcome::TurnErrorKind;
use praxis_loop::tool::PreparedToolCall;
use praxis_loop::tool::Tool;
use praxis_loop::tool::ToolCall as LoopToolCall;
use praxis_loop::tool::ToolExecutionContext;
use praxis_loop::tool::ToolLifecycleSink;
use praxis_loop::tool::ToolResult as LoopToolResult;
use praxis_loop::tool::ToolSpec as LoopToolSpec;
use std::sync::Arc;

use crate::error::PraxisErr;
use crate::tools::registry::ToolPreparation as HandlerPreparation;
use crate::tools::tool_call_runtime::ToolCallRuntime;

use super::tool_call_bridge::loop_tool_call_to_core_tool_call;
use super::tool_result_bridge::core_tool_description;
use super::tool_result_bridge::response_input_to_loop_tool_result;

pub(in crate::praxis::turn_loop_adapter) fn resolve_tool_from_runtime(
    runtime: &ToolCallRuntime,
    name: &str,
) -> Option<Arc<dyn Tool>> {
    let spec = runtime.find_spec(name)?;
    Some(Arc::new(PraxisLoopTool {
        runtime: runtime.clone(),
        spec: loop_tool_spec(name, &spec),
    }))
}

struct PraxisLoopTool {
    runtime: ToolCallRuntime,
    spec: LoopToolSpec,
}

#[async_trait]
impl Tool for PraxisLoopTool {
    fn spec(&self) -> LoopToolSpec {
        self.spec.clone()
    }

    async fn prepare(&self, call: &LoopToolCall) -> LoopResult<PreparedToolCall> {
        let core_call = loop_tool_call_to_core_tool_call(call.clone())?;
        let preparation = self
            .runtime
            .prepare_tool_call(core_call)
            .await
            .map_err(|err| TurnError::new(TurnErrorKind::Tool, err.to_string()))?;
        Ok(PreparedToolCall::new(preparation.effects().clone()).with_state(preparation))
    }

    async fn execute(
        &self,
        call: LoopToolCall,
        context: ToolExecutionContext,
    ) -> LoopResult<LoopToolResult> {
        let core_call = loop_tool_call_to_core_tool_call(call)?;

        let response = self
            .runtime
            .clone()
            .handle_tool_call_observed(core_call, context.cancel, context.effects)
            .await
            .map_err(loop_tool_error)?;

        Ok(response_input_to_loop_tool_result(response))
    }

    async fn execute_prepared_streaming(
        &self,
        call: LoopToolCall,
        mut prepared: PreparedToolCall,
        context: ToolExecutionContext,
        _lifecycle: &(dyn ToolLifecycleSink + Send + Sync),
    ) -> LoopResult<LoopToolResult> {
        let preparation = prepared.take_state::<HandlerPreparation>().ok_or_else(|| {
            TurnError::new(
                TurnErrorKind::Internal,
                "Praxis tool plan lost its prepared handler state",
            )
        })?;
        let core_call = loop_tool_call_to_core_tool_call(call)?;
        let response = self
            .runtime
            .clone()
            .handle_prepared_tool_call_observed(
                core_call,
                preparation,
                context.cancel,
                context.effects,
            )
            .await
            .map_err(loop_tool_error)?;
        Ok(response_input_to_loop_tool_result(response))
    }
}

fn loop_tool_error(err: PraxisErr) -> TurnError {
    TurnError::new(TurnErrorKind::Tool, err.to_string())
}

fn loop_tool_spec(name: &str, spec: &praxis_tools::ToolSpec) -> LoopToolSpec {
    LoopToolSpec {
        name: name.to_string(),
        description: core_tool_description(spec),
    }
}
