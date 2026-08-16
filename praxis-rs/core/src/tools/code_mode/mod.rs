#[cfg(feature = "code_mode")]
mod real;

#[cfg(feature = "code_mode")]
pub(crate) use real::CodeModeExecuteHandler;
#[cfg(feature = "code_mode")]
pub(crate) use real::CodeModeService;
#[cfg(feature = "code_mode")]
pub(crate) use real::CodeModeTurnWorker;
#[cfg(feature = "code_mode")]
pub(crate) use real::CodeModeWaitHandler;
#[cfg(feature = "code_mode")]
pub(crate) use real::is_code_mode_nested_tool;

#[cfg(not(feature = "code_mode"))]
mod disabled {
    use std::collections::HashMap;
    use std::sync::Arc;

    use async_trait::async_trait;
    use serde_json::Value as JsonValue;

    use crate::capabilities::ToolCapability;
    use crate::function_tool::FunctionCallError;
    use crate::praxis::Session;
    use crate::praxis::TurnContext;
    use crate::tools::context::FunctionToolOutput;
    use crate::tools::context::SharedTurnDiffTracker;
    use crate::tools::context::ToolInvocation;
    use crate::tools::context::ToolPayload;
    use crate::tools::registry::ToolHandler;
    use crate::tools::registry::ToolKind;

    pub(crate) type CodeModeTurnWorker = ();

    pub(crate) struct CodeModeService;

    impl CodeModeService {
        pub(crate) fn new() -> Self {
            Self
        }

        pub(crate) async fn stored_values(&self) -> HashMap<String, JsonValue> {
            HashMap::new()
        }

        pub(crate) async fn replace_stored_values(&self, _values: HashMap<String, JsonValue>) {}

        pub(crate) async fn start_turn_worker(
            &self,
            _session: &Arc<Session>,
            _turn: &Arc<TurnContext>,
            _router: ToolCapability,
            _tracker: SharedTurnDiffTracker,
        ) -> Option<CodeModeTurnWorker> {
            None
        }
    }

    pub(crate) struct CodeModeExecuteHandler;

    #[async_trait]
    impl ToolHandler for CodeModeExecuteHandler {
        type Output = FunctionToolOutput;

        fn kind(&self) -> ToolKind {
            ToolKind::Function
        }

        fn matches_kind(&self, payload: &ToolPayload) -> bool {
            matches!(payload, ToolPayload::Custom { .. })
        }

        async fn handle(
            &self,
            _invocation: ToolInvocation,
        ) -> Result<Self::Output, FunctionCallError> {
            Err(FunctionCallError::RespondToModel(
                "Code Mode is not compiled into this Praxis build".to_string(),
            ))
        }
    }

    pub(crate) struct CodeModeWaitHandler;

    #[async_trait]
    impl ToolHandler for CodeModeWaitHandler {
        type Output = FunctionToolOutput;

        fn kind(&self) -> ToolKind {
            ToolKind::Function
        }

        async fn handle(
            &self,
            _invocation: ToolInvocation,
        ) -> Result<Self::Output, FunctionCallError> {
            Err(FunctionCallError::RespondToModel(
                "Code Mode wait is not compiled into this Praxis build".to_string(),
            ))
        }
    }

    pub(crate) fn is_code_mode_nested_tool(tool_name: &str) -> bool {
        praxis_tools::is_code_mode_nested_tool(tool_name)
    }
}

#[cfg(not(feature = "code_mode"))]
pub(crate) use disabled::CodeModeExecuteHandler;
#[cfg(not(feature = "code_mode"))]
pub(crate) use disabled::CodeModeService;
#[cfg(not(feature = "code_mode"))]
pub(crate) use disabled::CodeModeTurnWorker;
#[cfg(not(feature = "code_mode"))]
pub(crate) use disabled::CodeModeWaitHandler;
#[cfg(not(feature = "code_mode"))]
pub(crate) use disabled::is_code_mode_nested_tool;
