use praxis_protocol::models::ResponseItem;
use std::collections::HashSet;
use tokio_util::sync::CancellationToken;

use crate::SkillLoadOutcome;
use crate::capabilities::ToolCapabilities;
use crate::capabilities::publish_tools;
use crate::error::PraxisErr;
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
) -> PraxisResult<ToolCapabilities> {
    if let Some(tools) = turn_context.tool_capabilities.get() {
        tracing::trace!(
            turn_id = %turn_context.sub_id,
            "reusing frozen turn tool capabilities"
        );
        return Ok(tools.clone());
    }
    let started_at = std::time::Instant::now();
    turn_context
        .tool_capabilities
        .get_or_try_init(|| async {
            build_and_publish_tools(
                sess,
                turn_context,
                input,
                explicitly_enabled_connectors,
                skills_outcome,
                cancellation_token,
            )
            .await
        })
        .await
        .cloned()
        .inspect(|_| {
            tracing::debug!(
                turn_id = %turn_context.sub_id,
                elapsed_ms = started_at.elapsed().as_millis(),
                "prepared frozen turn tool capabilities"
            );
        })
}

async fn build_and_publish_tools(
    sess: &Session,
    turn_context: &TurnContext,
    input: &[ResponseItem],
    explicitly_enabled_connectors: &HashSet<String>,
    skills_outcome: Option<&SkillLoadOutcome>,
    cancellation_token: &CancellationToken,
) -> PraxisResult<ToolCapabilities> {
    let mcp_snapshot = mcp_snapshot::load(sess, cancellation_token).await?;
    let tool_visibility_policy = visibility::resolve(sess, turn_context);
    let code_mode_router = ToolRouter::from_config(
        &turn_context.tools_config.for_code_mode_nested_tools(),
        ToolRouterParams {
            mcp_tools: Some(
                mcp_snapshot
                    .tools
                    .iter()
                    .map(|(name, tool)| (name.clone(), tool.tool.clone()))
                    .collect(),
            ),
            app_tools: None,
            discoverable_tools: None,
            dynamic_tools: turn_context.dynamic_tools.as_slice(),
            tool_visibility_policy: tool_visibility_policy.as_ref(),
        },
    );
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
    let model_router = ToolRouter::from_config(
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
    );

    publish_tools(
        &sess.services._capability_scope,
        sess.conversation_id,
        turn_context.sub_id.as_str(),
        model_router,
        code_mode_router,
    )
    .map_err(|error| {
        PraxisErr::Fatal(format!(
            "failed to publish turn tool capabilities: {error:#}"
        ))
    })
}
