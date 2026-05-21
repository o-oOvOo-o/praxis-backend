use std::path::PathBuf;

use futures::future::BoxFuture;
use praxis_protocol::models::PermissionProfile;
use praxis_protocol::protocol::ReviewDecision;
use serde::Serialize;

use crate::guardian::GuardianApprovalRequest;
use crate::guardian::review_approval_request;
use crate::guardian::routes_approval_to_guardian;
use crate::sandboxing::SandboxPermissions;
use crate::tools::sandboxing::ApprovalCtx;
use crate::tools::sandboxing::ExecApprovalRequirement;
use crate::tools::sandboxing::with_cached_approval;

#[derive(Clone, Copy, Debug)]
pub(crate) enum RuntimeApprovalKind {
    Shell,
    UnifiedExec { tty: bool },
}

#[derive(Clone, Debug)]
pub(crate) struct RuntimeApprovalPlan {
    pub(crate) tool_name: &'static str,
    pub(crate) kind: RuntimeApprovalKind,
    pub(crate) command: Vec<String>,
    pub(crate) cwd: PathBuf,
    pub(crate) sandbox_permissions: SandboxPermissions,
    pub(crate) additional_permissions: Option<PermissionProfile>,
    pub(crate) justification: Option<String>,
    pub(crate) exec_approval_requirement: ExecApprovalRequirement,
}

/// Shared shell/unified-exec approval bridge.
///
/// The two runtimes have different spawn backends, but their approval flow is
/// the same: guardian review when routed there, otherwise cache-aware user
/// command approval with the same network and execpolicy amendment context.
pub(crate) fn start_runtime_approval_async<'a, K>(
    keys: Vec<K>,
    plan: RuntimeApprovalPlan,
    ctx: ApprovalCtx<'a>,
) -> BoxFuture<'a, ReviewDecision>
where
    K: Serialize + Send + 'a,
{
    Box::pin(async move {
        let retry_reason = ctx.retry_reason.clone();
        let reason = retry_reason.clone().or(plan.justification.clone());
        let session = ctx.session;
        let turn = ctx.turn;
        let call_id = ctx.call_id.to_string();
        if routes_approval_to_guardian(turn) {
            let guardian_request = match plan.kind {
                RuntimeApprovalKind::Shell => GuardianApprovalRequest::Shell {
                    id: call_id,
                    command: plan.command,
                    cwd: plan.cwd,
                    sandbox_permissions: plan.sandbox_permissions,
                    additional_permissions: plan.additional_permissions,
                    justification: plan.justification,
                },
                RuntimeApprovalKind::UnifiedExec { tty } => GuardianApprovalRequest::ExecCommand {
                    id: call_id,
                    command: plan.command,
                    cwd: plan.cwd,
                    sandbox_permissions: plan.sandbox_permissions,
                    additional_permissions: plan.additional_permissions,
                    justification: plan.justification,
                    tty,
                },
            };
            return review_approval_request(session, turn, guardian_request, retry_reason).await;
        }

        let tool_name = plan.tool_name;
        let command = plan.command;
        let cwd = plan.cwd;
        let additional_permissions = plan.additional_permissions;
        let proposed_execpolicy_amendment = plan
            .exec_approval_requirement
            .proposed_execpolicy_amendment()
            .cloned();

        with_cached_approval(&session.services, tool_name, keys, move || async move {
            let available_decisions = None;
            session
                .request_command_approval(
                    turn,
                    call_id,
                    /*approval_id*/ None,
                    command,
                    cwd,
                    reason,
                    ctx.network_approval_context.clone(),
                    proposed_execpolicy_amendment,
                    additional_permissions,
                    available_decisions,
                )
                .await
        })
        .await
    })
}
