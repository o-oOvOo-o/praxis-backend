use async_trait::async_trait;
use praxis_loop::outcome::LoopResult;
use praxis_loop::outcome::TurnError;
use praxis_loop::outcome::TurnErrorKind;
use praxis_loop::tool::ConcurrencyMode;
use praxis_loop::tool::Tool;
use praxis_loop::tool::ToolCall as LoopToolCall;
use praxis_loop::tool::ToolResult as LoopToolResult;
use praxis_loop::tool::ToolSpec as LoopToolSpec;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

use crate::error::PraxisErr;
use crate::tools::tool_call_runtime::ToolCallRuntime;

use super::tool_call_bridge::loop_tool_call_to_core_tool_call;
use super::tool_result_bridge::core_tool_description;
use super::tool_result_bridge::response_input_to_loop_tool_result;

pub(super) fn resolve_tool_from_runtime(
    runtime: &ToolCallRuntime,
    name: &str,
) -> Option<Arc<dyn Tool>> {
    let spec = runtime.find_spec(name)?;
    Some(Arc::new(PraxisLoopTool {
        runtime: runtime.clone(),
        spec: loop_tool_spec(name, &spec, runtime.tool_concurrency_mode(name)),
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

    fn concurrency(&self) -> ConcurrencyMode {
        self.spec.concurrency
    }

    async fn execute(
        &self,
        call: LoopToolCall,
        cancel: CancellationToken,
    ) -> LoopResult<LoopToolResult> {
        let core_call = loop_tool_call_to_core_tool_call(call)?;

        let response = self
            .runtime
            .clone()
            .handle_tool_call(core_call, cancel)
            .await
            .map_err(loop_tool_error)?;

        Ok(response_input_to_loop_tool_result(response))
    }
}

fn loop_tool_error(err: PraxisErr) -> TurnError {
    TurnError::new(TurnErrorKind::Tool, err.to_string())
}

fn loop_tool_spec(
    name: &str,
    spec: &praxis_tools::ToolSpec,
    concurrency: ConcurrencyMode,
) -> LoopToolSpec {
    LoopToolSpec {
        name: name.to_string(),
        description: core_tool_description(spec),
        concurrency,
    }
}
