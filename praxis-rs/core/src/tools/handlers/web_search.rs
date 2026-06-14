use async_trait::async_trait;
use praxis_protocol::models::WebSearchAction;

use crate::function_tool::FunctionCallError;
use crate::tools::context::FunctionToolOutput;
use crate::tools::context::ToolInvocation;
use crate::tools::context::ToolPayload;
use crate::tools::events::ToolEventCtx;
use crate::tools::events::ToolLifecycleEmitter;
use crate::tools::handlers::parse_arguments;
use crate::tools::registry::ToolHandler;
use crate::tools::registry::ToolKind;
use crate::web_search::RipWebSearchArgs;
use crate::web_search::rip_web_search;

pub struct WebSearchHandler;

#[async_trait]
impl ToolHandler for WebSearchHandler {
    type Output = FunctionToolOutput;

    fn kind(&self) -> ToolKind {
        ToolKind::Function
    }

    async fn handle(&self, invocation: ToolInvocation) -> Result<Self::Output, FunctionCallError> {
        let ToolInvocation {
            session,
            turn,
            call_id,
            payload,
            ..
        } = invocation;

        let arguments = match payload {
            ToolPayload::Function { arguments } => arguments,
            _ => {
                return Err(FunctionCallError::Fatal(
                    "web_search handler received unsupported payload".to_string(),
                ));
            }
        };
        let args: RipWebSearchArgs = parse_arguments(&arguments)?;
        let Some(display_query) = args.primary_query() else {
            return Err(FunctionCallError::RespondToModel(
                "web_search requires a non-empty `query` or at least one non-empty item in `queries`"
                    .to_string(),
            ));
        };

        let action = WebSearchAction::Search {
            query: Some(display_query.clone()),
            queries: args.queries.clone(),
        };
        let tool_events = ToolLifecycleEmitter::new(ToolEventCtx::new(
            session.as_ref(),
            turn.as_ref(),
            &call_id,
            None,
        ));
        tool_events.web_search_begin().await;

        let response = rip_web_search(args).await;
        tool_events.web_search_end(display_query, action).await;

        let content = serde_json::to_string_pretty(&response).map_err(|err| {
            FunctionCallError::Fatal(format!("failed to serialize web_search response: {err}"))
        })?;
        Ok(FunctionToolOutput::from_text(content, Some(true)))
    }
}
