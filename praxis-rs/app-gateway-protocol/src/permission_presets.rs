use praxis_protocol::protocol::SandboxPolicy as CoreSandboxPolicy;
use praxis_utils_approval_presets::PermissionPreset;

use crate::{ApprovalsReviewer, AskForApproval, ConfigRequirements, SandboxMode, SandboxPolicy};

#[derive(Debug, Clone, PartialEq)]
pub struct PermissionPresetConfig {
    pub approval_policy: AskForApproval,
    pub approvals_reviewer: ApprovalsReviewer,
    pub sandbox_mode: Option<SandboxMode>,
    pub sandbox_policy: SandboxPolicy,
}

pub fn permission_preset_config(preset: PermissionPreset) -> PermissionPresetConfig {
    let approval_preset = preset.approval_preset();
    let sandbox_mode = match &approval_preset.sandbox {
        CoreSandboxPolicy::ReadOnly { .. } => Some(SandboxMode::ReadOnly),
        CoreSandboxPolicy::WorkspaceWrite { .. } => Some(SandboxMode::WorkspaceWrite),
        CoreSandboxPolicy::DangerFullAccess => Some(SandboxMode::DangerFullAccess),
        CoreSandboxPolicy::ExternalSandbox { .. } => None,
    };
    PermissionPresetConfig {
        approval_policy: approval_preset.approval.into(),
        approvals_reviewer: preset.approvals_reviewer().into(),
        sandbox_mode,
        sandbox_policy: approval_preset.sandbox.into(),
    }
}

pub fn available_permission_presets(
    requirements: Option<&ConfigRequirements>,
    guardian_approval_enabled: bool,
) -> Vec<PermissionPreset> {
    PermissionPreset::ALL
        .into_iter()
        .filter(|preset| {
            if *preset == PermissionPreset::GuardianApprovals && !guardian_approval_enabled {
                return false;
            }
            let config = permission_preset_config(*preset);
            let Some(sandbox_mode) = config.sandbox_mode else {
                return false;
            };
            requirements.is_none_or(|requirements| {
                requirements
                    .allowed_approval_policies
                    .as_ref()
                    .is_none_or(|allowed| allowed.contains(&config.approval_policy))
                    && requirements
                        .allowed_sandbox_modes
                        .as_ref()
                        .is_none_or(|allowed| allowed.contains(&sandbox_mode))
            })
        })
        .collect()
}

pub fn permission_preset_matches_config(
    approval_policy: AskForApproval,
    approvals_reviewer: ApprovalsReviewer,
    sandbox_policy: &SandboxPolicy,
    preset: PermissionPreset,
) -> bool {
    let config = permission_preset_config(preset);
    approval_policy == config.approval_policy
        && approvals_reviewer == config.approvals_reviewer
        && *sandbox_policy == config.sandbox_policy
}
