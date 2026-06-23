use std::collections::HashSet;
use std::sync::Arc;

use praxis_protocol::models::ResponseItem;
use tokio_util::sync::CancellationToken;

use crate::SkillLoadOutcome;
use crate::error::Result as PraxisResult;
use crate::praxis::Session;
use crate::praxis::TurnContext;
use crate::tools::ToolRouter;
use crate::tools::router::ToolRouterParams;

mod app_tool_exposure;
mod connector_context;
mod mcp_selection;
mod mcp_snapshot;
mod visibility;

pub(crate) async fn built_tools(
    sess: &Session,
    turn_context: &TurnContext,
    input: &[ResponseItem],
    explicitly_enabled_connectors: &HashSet<String>,
    skills_outcome: Option<&SkillLoadOutcome>,
    cancellation_token: &CancellationToken,
) -> PraxisResult<Arc<ToolRouter>> {
    let mcp_snapshot = mcp_snapshot::load(sess, cancellation_token).await?;
    let connector_context = connector_context::build(
        sess,
        turn_context,
        &mcp_snapshot.tools,
        explicitly_enabled_connectors,
    )
    .await;
    let selected_mcp_tools = mcp_selection::select(
        mcp_snapshot.tools,
        &connector_context,
        input,
        skills_outcome,
        turn_context,
    );
    let tool_exposure = app_tool_exposure::apply(
        selected_mcp_tools,
        connector_context.app_tools,
        turn_context,
    );
    let tool_visibility_policy = visibility::resolve(sess, turn_context);

    Ok(Arc::new(ToolRouter::from_config(
        &turn_context.tools_config,
        ToolRouterParams {
            mcp_tools: mcp_snapshot.has_mcp_servers.then(|| {
                tool_exposure
                    .mcp_tools
                    .into_iter()
                    .map(|(name, tool)| (name, tool.tool))
                    .collect()
            }),
            app_tools: tool_exposure.app_tools,
            discoverable_tools: connector_context.discoverable_tools,
            dynamic_tools: turn_context.dynamic_tools.as_slice(),
            tool_visibility_policy: tool_visibility_policy.as_ref(),
        },
    )))
}
