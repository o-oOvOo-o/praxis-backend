use crate::state::ResolvedTurnPermissions;
use praxis_protocol::permissions::FileSystemSandboxKind;
use praxis_protocol::protocol::AskForApproval;
use praxis_protocol::protocol::SandboxPolicy;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PermissionConflict {
    #[error("full-access sandbox still allows approval prompts")]
    FullAccessStillPrompts,
    #[error("approval prompts disabled while sandbox can still deny execution")]
    PromptlessRestrictedSandbox,
    #[error("legacy sandbox policy and filesystem policy disagree")]
    SandboxProjectionDrift,
}

pub fn detect_permission_conflicts(
    permissions: &ResolvedTurnPermissions,
) -> Vec<PermissionConflict> {
    let mut conflicts = Vec::new();
    if matches!(permissions.sandbox_policy, SandboxPolicy::DangerFullAccess)
        && permissions.approval_policy != AskForApproval::Never
    {
        conflicts.push(PermissionConflict::FullAccessStillPrompts);
    }

    if permissions.approval_policy == AskForApproval::Never
        && matches!(
            permissions.file_system_sandbox_policy.kind,
            FileSystemSandboxKind::Restricted
        )
        && !matches!(
            permissions.sandbox_policy,
            SandboxPolicy::DangerFullAccess | SandboxPolicy::ExternalSandbox { .. }
        )
    {
        conflicts.push(PermissionConflict::PromptlessRestrictedSandbox);
    }

    if matches!(permissions.sandbox_policy, SandboxPolicy::DangerFullAccess)
        && permissions.file_system_sandbox_policy.kind != FileSystemSandboxKind::Unrestricted
    {
        conflicts.push(PermissionConflict::SandboxProjectionDrift);
    }

    conflicts
}
