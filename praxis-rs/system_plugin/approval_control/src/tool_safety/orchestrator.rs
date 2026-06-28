use crate::ApprovalDecision;
use crate::ApprovalRequest;
use crate::state::ResolvedTurnPermissions;
use crate::tool_safety::SafetyToolKind;
use crate::tool_safety::SandboxExecutionPlan;
use praxis_protocol::protocol::AskForApproval;

#[derive(Debug, Clone)]
pub struct ToolSafetyRequest<'a> {
    pub id: &'a str,
    pub thread_id: Option<&'a str>,
    pub turn_id: Option<&'a str>,
    pub kind: SafetyToolKind,
    pub permissions: &'a ResolvedTurnPermissions,
    pub approval_required: bool,
    pub reason: Option<&'a str>,
}

#[derive(Debug, Clone, Default)]
pub struct ToolSafetyOrchestrator;

impl ToolSafetyOrchestrator {
    pub fn decide(&self, request: ToolSafetyRequest<'_>) -> ApprovalDecision {
        let permissions = request.permissions.clone().normalized();
        let plan = SandboxExecutionPlan::from_permissions(
            permissions.sandbox_policy.clone(),
            permissions.file_system_sandbox_policy.clone(),
            permissions.network_sandbox_policy,
            permissions.windows_sandbox_level,
        );

        if permissions.is_promptless_full_access() {
            return ApprovalDecision::Run { plan };
        }

        if !request.approval_required {
            return ApprovalDecision::Run { plan };
        }

        match permissions.approval_policy {
            AskForApproval::Never => {
                ApprovalDecision::deny(request.reason.unwrap_or("approval is disabled"))
            }
            AskForApproval::UnlessTrusted
            | AskForApproval::OnFailure
            | AskForApproval::OnRequest
            | AskForApproval::Granular(_) => ApprovalDecision::AskUser {
                request: build_approval_request(request, permissions.generation),
            },
        }
    }
}

fn build_approval_request(
    request: ToolSafetyRequest<'_>,
    permissions_generation: u64,
) -> ApprovalRequest {
    let mut approval_request =
        ApprovalRequest::new(request.id.to_string(), request.kind, permissions_generation);
    if let Some(thread_id) = request.thread_id {
        approval_request = approval_request.with_thread_id(thread_id);
    }
    if let Some(turn_id) = request.turn_id {
        approval_request = approval_request.with_turn_id(turn_id);
    }
    if let Some(reason) = request.reason {
        approval_request = approval_request.with_reason(reason);
    }
    approval_request
}
