use crate::state::PermissionStateSource;
use crate::state::ThreadPermissionState;
use praxis_protocol::config_types::ApprovalsReviewer;
use praxis_protocol::config_types::WindowsSandboxLevel;
use praxis_protocol::permissions::FileSystemSandboxPolicy;
use praxis_protocol::permissions::NetworkSandboxPolicy;
use praxis_protocol::protocol::AskForApproval;
use praxis_protocol::protocol::SandboxPolicy;
use serde::Deserialize;
use serde::Serialize;
use std::path::Path;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PermissionOverride {
    pub approval_policy: Option<AskForApproval>,
    pub approvals_reviewer: Option<ApprovalsReviewer>,
    pub sandbox_policy: Option<SandboxPolicy>,
    pub windows_sandbox_level: Option<WindowsSandboxLevel>,
}

impl PermissionOverride {
    pub fn is_empty(&self) -> bool {
        self.approval_policy.is_none()
            && self.approvals_reviewer.is_none()
            && self.sandbox_policy.is_none()
            && self.windows_sandbox_level.is_none()
    }
}

pub fn apply_permission_override(
    state: &ThreadPermissionState,
    override_state: &PermissionOverride,
    cwd: &Path,
) -> ThreadPermissionState {
    if override_state.is_empty() {
        return state.clone();
    }

    let mut next = state.clone().bump(PermissionStateSource::RuntimeOverride);
    if let Some(approval_policy) = override_state.approval_policy {
        next.approval_policy = approval_policy;
    }
    if let Some(approvals_reviewer) = override_state.approvals_reviewer {
        next.approvals_reviewer = approvals_reviewer;
    }
    if let Some(sandbox_policy) = override_state.sandbox_policy.clone() {
        next.network_sandbox_policy = NetworkSandboxPolicy::from(&sandbox_policy);
        next.file_system_sandbox_policy =
            FileSystemSandboxPolicy::from_legacy_sandbox_policy(&sandbox_policy, cwd);
        next.sandbox_policy = sandbox_policy;
    }
    if let Some(windows_sandbox_level) = override_state.windows_sandbox_level {
        next.windows_sandbox_level = windows_sandbox_level;
    }
    next
}
