use crate::state::ResolvedTurnPermissions;
use crate::tool_safety::SafetyToolKind;
use praxis_protocol::protocol::AskForApproval;
use serde::Deserialize;
use serde::Serialize;

#[derive(Debug, Clone, Copy)]
pub struct SandboxRetryRequest<'a> {
    pub kind: SafetyToolKind,
    pub permissions: &'a ResolvedTurnPermissions,
    pub tool_allows_no_sandbox_approval: bool,
    pub tool_bypasses_retry_approval: bool,
    pub network_retry_available: bool,
    pub network_retry_requires_approval: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SandboxRetryPolicy {
    pub allow_without_sandbox: bool,
    pub ask_before_retry: bool,
}

impl SandboxRetryPolicy {
    pub fn for_denied_sandbox(request: SandboxRetryRequest<'_>) -> Self {
        let permissions = request.permissions.clone().normalized();
        if permissions.is_promptless_full_access() {
            return Self {
                allow_without_sandbox: true,
                ask_before_retry: false,
            };
        }

        let network_policy_prompt =
            matches!(permissions.approval_policy, AskForApproval::OnRequest)
                && request.network_retry_available
                && request.network_retry_requires_approval;
        let allow_without_sandbox =
            request.tool_allows_no_sandbox_approval || network_policy_prompt;
        if !allow_without_sandbox {
            return Self {
                allow_without_sandbox: false,
                ask_before_retry: false,
            };
        }

        if matches!(permissions.approval_policy, AskForApproval::Never) {
            return Self {
                allow_without_sandbox: false,
                ask_before_retry: false,
            };
        }

        let ask_before_retry = match request.kind {
            SafetyToolKind::Exec | SafetyToolKind::ApplyPatch | SafetyToolKind::Network => {
                request.network_retry_available || !request.tool_bypasses_retry_approval
            }
            SafetyToolKind::Mcp | SafetyToolKind::Permission | SafetyToolKind::Unknown => true,
        };

        Self {
            allow_without_sandbox,
            ask_before_retry,
        }
    }
}
