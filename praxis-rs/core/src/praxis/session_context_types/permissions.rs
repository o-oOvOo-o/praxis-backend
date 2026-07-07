use std::fmt::Debug;

use praxis_protocol::config_types::ApprovalsReviewer;
use praxis_protocol::config_types::WindowsSandboxLevel;
use praxis_protocol::models::PermissionProfile;
use praxis_protocol::permissions::FileSystemSandboxPolicy;
use praxis_protocol::permissions::NetworkSandboxPolicy;
use praxis_protocol::protocol::AskForApproval;
use praxis_protocol::protocol::SandboxPolicy;
use praxis_system_plugin_approval_control::PermissionHandle;
use praxis_system_plugin_approval_control::PermissionStateSource;
use praxis_system_plugin_approval_control::ResolvedTurnPermissions;
use praxis_system_plugin_approval_control::ThreadPermissionState;

use crate::config::Constrained;

use super::super::SessionConfiguration;

#[derive(Clone, Debug)]
pub(crate) struct EffectivePermissions {
    pub(crate) approval_policy: Constrained<AskForApproval>,
    pub(crate) approvals_reviewer: ApprovalsReviewer,
    pub(crate) sandbox_policy: Constrained<SandboxPolicy>,
    pub(crate) file_system_sandbox_policy: FileSystemSandboxPolicy,
    pub(crate) network_sandbox_policy: NetworkSandboxPolicy,
    pub(crate) windows_sandbox_level: WindowsSandboxLevel,
    pub(crate) granted_permissions: Option<PermissionProfile>,
    pub(crate) generation: u64,
}

impl EffectivePermissions {
    pub(crate) fn from_resolved_turn_permissions(permissions: ResolvedTurnPermissions) -> Self {
        Self {
            approval_policy: Constrained::allow_any(permissions.approval_policy),
            approvals_reviewer: permissions.approvals_reviewer,
            sandbox_policy: Constrained::allow_any(permissions.sandbox_policy),
            file_system_sandbox_policy: permissions.file_system_sandbox_policy,
            network_sandbox_policy: permissions.network_sandbox_policy,
            windows_sandbox_level: permissions.windows_sandbox_level,
            granted_permissions: permissions.granted_permissions,
            generation: permissions.generation,
        }
    }

    pub(crate) fn as_resolved_turn_permissions(&self) -> ResolvedTurnPermissions {
        ResolvedTurnPermissions {
            approval_policy: self.approval_policy.value(),
            approvals_reviewer: self.approvals_reviewer,
            sandbox_policy: self.sandbox_policy.get().clone(),
            file_system_sandbox_policy: self.file_system_sandbox_policy.clone(),
            network_sandbox_policy: self.network_sandbox_policy,
            windows_sandbox_level: self.windows_sandbox_level,
            granted_permissions: self.granted_permissions.clone(),
            generation: self.generation,
        }
    }
}

#[derive(Clone)]
pub(crate) struct LiveEffectivePermissions {
    handle: PermissionHandle,
}

impl LiveEffectivePermissions {
    pub(crate) fn new(handle: PermissionHandle) -> Self {
        Self { handle }
    }

    pub(crate) fn snapshot(&self) -> EffectivePermissions {
        EffectivePermissions::from_resolved_turn_permissions(self.handle.current())
    }
}

impl Debug for LiveEffectivePermissions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LiveEffectivePermissions")
            .field("snapshot", &self.snapshot())
            .finish()
    }
}

pub(crate) fn thread_permissions_from_session_configuration(
    session_configuration: &SessionConfiguration,
) -> ThreadPermissionState {
    ThreadPermissionState::new(
        None,
        PermissionStateSource::Config,
        session_configuration.approval_policy.value(),
        session_configuration.approvals_reviewer,
        session_configuration.sandbox_policy.get().clone(),
        session_configuration.file_system_sandbox_policy.clone(),
        session_configuration.network_sandbox_policy,
        session_configuration.windows_sandbox_level,
    )
    .normalized()
}
