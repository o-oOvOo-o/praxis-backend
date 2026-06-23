use super::PermissionOverride;
use super::apply_permission_override;
use crate::state::PermissionPreset;
use crate::state::PermissionStateSource;
use crate::state::ThreadPermissionState;
use praxis_protocol::config_types::ApprovalsReviewer;
use praxis_protocol::config_types::WindowsSandboxLevel;
use praxis_protocol::permissions::FileSystemSandboxPolicy;
use praxis_protocol::permissions::NetworkSandboxPolicy;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct PermissionResolver {
    default_reviewer: ApprovalsReviewer,
    default_windows_sandbox_level: WindowsSandboxLevel,
}

impl PermissionResolver {
    pub fn new(
        default_reviewer: ApprovalsReviewer,
        default_windows_sandbox_level: WindowsSandboxLevel,
    ) -> Self {
        Self {
            default_reviewer,
            default_windows_sandbox_level,
        }
    }

    pub fn from_preset(
        &self,
        preset: PermissionPreset,
        cwd: &Path,
        override_state: Option<&PermissionOverride>,
    ) -> ThreadPermissionState {
        let sandbox_policy = preset.sandbox_policy();
        let base = ThreadPermissionState::new(
            None,
            PermissionStateSource::Preset,
            preset.approval_policy(),
            self.default_reviewer,
            sandbox_policy.clone(),
            FileSystemSandboxPolicy::from_legacy_sandbox_policy(&sandbox_policy, cwd),
            NetworkSandboxPolicy::from(&sandbox_policy),
            self.default_windows_sandbox_level,
        );

        match override_state {
            Some(override_state) => apply_permission_override(&base, override_state, cwd),
            None => base,
        }
    }
}
