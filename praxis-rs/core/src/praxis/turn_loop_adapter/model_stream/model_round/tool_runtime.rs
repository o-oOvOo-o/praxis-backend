use std::collections::HashSet;
use std::sync::Arc;

use praxis_protocol::models::ResponseItem;
use tokio_util::sync::CancellationToken;

use crate::SkillLoadOutcome;
use crate::error::Result as PraxisResult;
use crate::praxis::Session;
use crate::praxis::TurnContext;
use crate::tools::ToolRouter;
use crate::tools::context::SharedTurnDiffTracker;
use crate::tools::tool_call_runtime::ToolCallRuntime;

use super::super::super::super::model_request::built_tools;

pub(super) struct ModelRoundTools {
    router: Arc<ToolRouter>,
    runtime: ToolCallRuntime,
}

impl ModelRoundTools {
    pub(super) fn router(&self) -> &ToolRouter {
        self.router.as_ref()
    }

    pub(super) fn router_arc(&self) -> Arc<ToolRouter> {
        Arc::clone(&self.router)
    }

    pub(super) fn runtime(&self) -> ToolCallRuntime {
        self.runtime.clone()
    }
}

pub(super) async fn build_tool_runtime(
    sess: Arc<Session>,
    turn_context: Arc<TurnContext>,
    turn_diff_tracker: SharedTurnDiffTracker,
    input: &[ResponseItem],
    explicitly_enabled_connectors: &HashSet<String>,
    skills_outcome: Option<&SkillLoadOutcome>,
    cancellation_token: &CancellationToken,
) -> PraxisResult<ModelRoundTools> {
    let router = built_tools(
        sess.as_ref(),
        turn_context.as_ref(),
        input,
        explicitly_enabled_connectors,
        skills_outcome,
        cancellation_token,
    )
    .await?;
    let runtime = ToolCallRuntime::new(
        Arc::clone(&router),
        Arc::clone(&sess),
        Arc::clone(&turn_context),
        Arc::clone(&turn_diff_tracker),
    );

    Ok(ModelRoundTools { router, runtime })
}
