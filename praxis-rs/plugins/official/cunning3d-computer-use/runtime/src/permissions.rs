use metra_computer_use::{
    ComputerUseAction, ComputerUseError, ComputerUseErrorKind, ComputerUseResult,
    ComputerUseRiskClass,
};
use praxis_utils_approval_presets::PermissionPreset;
use serde_json::json;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PerCallApproval {
    #[default]
    NotGranted,
    Granted,
}

pub fn authorize_computer_use_action(
    permission_preset: PermissionPreset,
    approval: PerCallApproval,
    action: &ComputerUseAction,
) -> ComputerUseResult<()> {
    let risk = action.risk();
    match permission_preset {
        PermissionPreset::FullAccess => Ok(()),
        PermissionPreset::ReadOnly if !matches!(risk, ComputerUseRiskClass::Observe) => {
            Err(authorization_error(
                ComputerUseErrorKind::PermissionDenied,
                "Read Only permits observation but rejects UI interaction",
                permission_preset,
                action,
            ))
        }
        PermissionPreset::Default | PermissionPreset::GuardianApprovals
            if matches!(
                risk,
                ComputerUseRiskClass::SensitiveInput | ComputerUseRiskClass::ExternalEffect
            ) && approval != PerCallApproval::Granted =>
        {
            Err(authorization_error(
                ComputerUseErrorKind::ApprovalRequired,
                "this computer-use action requires approval for this invocation",
                permission_preset,
                action,
            ))
        }
        PermissionPreset::ReadOnly
        | PermissionPreset::Default
        | PermissionPreset::GuardianApprovals => Ok(()),
    }
}

fn authorization_error(
    kind: ComputerUseErrorKind,
    message: &str,
    permission_preset: PermissionPreset,
    action: &ComputerUseAction,
) -> ComputerUseError {
    ComputerUseError::new(kind, message).with_details(json!({
        "permission_preset": permission_preset.id(),
        "risk": action.risk(),
        "capability": action.capability(),
        "approval_scope": "single_invocation"
    }))
}
