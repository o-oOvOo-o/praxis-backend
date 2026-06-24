use crate::state::ResolvedTurnPermissions;
use crate::tool_safety::ToolKind;
use praxis_protocol::protocol::AskForApproval;
use serde::Deserialize;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SandboxRetryPolicy {
    pub allow_without_sandbox: bool,
    pub ask_before_retry: bool,
}

impl SandboxRetryPolicy {
    pub fn for_denied_sandbox(kind: ToolKind, permissions: &ResolvedTurnPermissions) -> Self {
        if permissions.is_promptless_full_access() {
            return Self {
                allow_without_sandbox: true,
                ask_before_retry: false,
            };
        }

        let ask_before_retry = !matches!(permissions.approval_policy, AskForApproval::Never);
        let allow_without_sandbox = match kind {
            ToolKind::Exec | ToolKind::ApplyPatch | ToolKind::Network => ask_before_retry,
            ToolKind::Mcp | ToolKind::Permission | ToolKind::Unknown => false,
        };

        Self {
            allow_without_sandbox,
            ask_before_retry,
        }
    }
}
