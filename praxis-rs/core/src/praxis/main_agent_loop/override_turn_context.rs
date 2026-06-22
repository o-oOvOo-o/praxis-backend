use std::path::PathBuf;
use std::sync::Arc;

use praxis_protocol::config_types::ApprovalsReviewer;
use praxis_protocol::config_types::CollaborationMode;
use praxis_protocol::config_types::Personality;
use praxis_protocol::config_types::ReasoningSummary as ReasoningSummaryConfig;
use praxis_protocol::config_types::ServiceTier;
use praxis_protocol::config_types::WindowsSandboxLevel;
use praxis_protocol::openai_models::ReasoningEffort as ReasoningEffortConfig;
use praxis_protocol::protocol::AskForApproval;
use praxis_protocol::protocol::Op;
use praxis_protocol::protocol::PraxisErrorInfo;
use praxis_protocol::protocol::SandboxPolicy;

use crate::praxis::Session;
use crate::praxis::SessionSettingsUpdate;

struct OverrideTurnContextUpdate {
    cwd: Option<PathBuf>,
    approval_policy: Option<AskForApproval>,
    approvals_reviewer: Option<ApprovalsReviewer>,
    sandbox_policy: Option<SandboxPolicy>,
    windows_sandbox_level: Option<WindowsSandboxLevel>,
    model_provider: Option<String>,
    model: Option<String>,
    effort: Option<Option<ReasoningEffortConfig>>,
    summary: Option<ReasoningSummaryConfig>,
    service_tier: Option<Option<ServiceTier>>,
    collaboration_mode: Option<CollaborationMode>,
    personality: Option<Personality>,
}

pub(super) async fn handle(sess: &Arc<Session>, sub_id: String, op: Op) {
    let Op::OverrideTurnContext {
        cwd,
        approval_policy,
        approvals_reviewer,
        sandbox_policy,
        windows_sandbox_level,
        model_provider,
        model,
        effort,
        summary,
        service_tier,
        collaboration_mode,
        personality,
    } = op
    else {
        return;
    };
    let update = OverrideTurnContextUpdate {
        cwd,
        approval_policy,
        approvals_reviewer,
        sandbox_policy,
        windows_sandbox_level,
        model_provider,
        model,
        effort,
        summary,
        service_tier,
        collaboration_mode,
        personality,
    };
    let collaboration_mode = if let Some(collab_mode) = update.collaboration_mode {
        collab_mode
    } else {
        let state = sess.state.lock().await;
        state.session_configuration.collaboration_mode.with_updates(
            update.model.clone(),
            update.effort,
            /*developer_instructions*/ None,
        )
    };
    if let Err(err) = sess
        .update_settings(SessionSettingsUpdate {
            cwd: update.cwd,
            approval_policy: update.approval_policy,
            approvals_reviewer: update.approvals_reviewer,
            sandbox_policy: update.sandbox_policy,
            windows_sandbox_level: update.windows_sandbox_level,
            model_provider: update.model_provider,
            collaboration_mode: Some(collaboration_mode),
            reasoning_summary: update.summary,
            service_tier: update.service_tier,
            personality: update.personality,
            ..Default::default()
        })
        .await
    {
        sess.raw_event_emitter(sub_id)
            .error(err.to_string(), Some(PraxisErrorInfo::BadRequest))
            .await;
    }
}
