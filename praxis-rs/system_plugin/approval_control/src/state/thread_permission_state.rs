use super::ResolvedTurnPermissions;
use praxis_protocol::config_types::ApprovalsReviewer;
use praxis_protocol::config_types::WindowsSandboxLevel;
use praxis_protocol::models::FileSystemPermissions;
use praxis_protocol::models::NetworkPermissions;
use praxis_protocol::models::PermissionProfile;
use praxis_protocol::permissions::FileSystemSandboxPolicy;
use praxis_protocol::permissions::NetworkSandboxPolicy;
use praxis_protocol::protocol::AskForApproval;
use praxis_protocol::protocol::SandboxPolicy;
use serde::Deserialize;
use serde::Serialize;
use std::collections::HashSet;
use std::hash::Hash;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionStateSource {
    Config,
    Preset,
    RuntimeOverride,
    Resume,
    Fork,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThreadPermissionState {
    pub thread_id: Option<String>,
    pub generation: u64,
    pub source: PermissionStateSource,
    pub approval_policy: AskForApproval,
    pub approvals_reviewer: ApprovalsReviewer,
    pub sandbox_policy: SandboxPolicy,
    pub file_system_sandbox_policy: FileSystemSandboxPolicy,
    pub network_sandbox_policy: NetworkSandboxPolicy,
    pub windows_sandbox_level: WindowsSandboxLevel,
    pub granted_session_permissions: Option<PermissionProfile>,
    pub granted_turn_permissions: Option<PermissionProfile>,
}

impl ThreadPermissionState {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        thread_id: Option<String>,
        source: PermissionStateSource,
        approval_policy: AskForApproval,
        approvals_reviewer: ApprovalsReviewer,
        sandbox_policy: SandboxPolicy,
        file_system_sandbox_policy: FileSystemSandboxPolicy,
        network_sandbox_policy: NetworkSandboxPolicy,
        windows_sandbox_level: WindowsSandboxLevel,
    ) -> Self {
        Self {
            thread_id,
            generation: 0,
            source,
            approval_policy,
            approvals_reviewer,
            sandbox_policy,
            file_system_sandbox_policy,
            network_sandbox_policy,
            windows_sandbox_level,
            granted_session_permissions: None,
            granted_turn_permissions: None,
        }
    }

    pub fn with_thread_id(mut self, thread_id: impl Into<String>) -> Self {
        self.thread_id = Some(thread_id.into());
        self
    }

    pub fn bump(mut self, source: PermissionStateSource) -> Self {
        self.generation = self.generation.saturating_add(1);
        self.source = source;
        self
    }

    pub fn resolved(&self) -> ResolvedTurnPermissions {
        ResolvedTurnPermissions {
            approval_policy: self.approval_policy,
            approvals_reviewer: self.approvals_reviewer,
            sandbox_policy: self.sandbox_policy.clone(),
            file_system_sandbox_policy: self.file_system_sandbox_policy.clone(),
            network_sandbox_policy: self.network_sandbox_policy,
            windows_sandbox_level: self.windows_sandbox_level,
            granted_permissions: merge_permission_profiles(
                self.granted_session_permissions.as_ref(),
                self.granted_turn_permissions.as_ref(),
            ),
            generation: self.generation,
        }
        .normalized()
    }

    pub fn normalized(mut self) -> Self {
        let resolved = self.resolved();
        self.approval_policy = resolved.approval_policy;
        self.approvals_reviewer = resolved.approvals_reviewer;
        self.sandbox_policy = resolved.sandbox_policy;
        self.file_system_sandbox_policy = resolved.file_system_sandbox_policy;
        self.network_sandbox_policy = resolved.network_sandbox_policy;
        self.windows_sandbox_level = resolved.windows_sandbox_level;
        self.granted_session_permissions = self
            .granted_session_permissions
            .filter(|permissions| !permissions.is_empty());
        self.granted_turn_permissions = self
            .granted_turn_permissions
            .filter(|permissions| !permissions.is_empty());
        self.generation = resolved.generation;
        self
    }

    pub fn same_effective_permissions(&self, other: &Self) -> bool {
        self.approval_policy == other.approval_policy
            && self.approvals_reviewer == other.approvals_reviewer
            && self.sandbox_policy == other.sandbox_policy
            && self.file_system_sandbox_policy == other.file_system_sandbox_policy
            && self.network_sandbox_policy == other.network_sandbox_policy
            && self.windows_sandbox_level == other.windows_sandbox_level
            && self.granted_session_permissions == other.granted_session_permissions
            && self.granted_turn_permissions == other.granted_turn_permissions
    }
}

pub(crate) fn merge_permission_profiles(
    base: Option<&PermissionProfile>,
    permissions: Option<&PermissionProfile>,
) -> Option<PermissionProfile> {
    let Some(permissions) = permissions else {
        return base.cloned();
    };

    match base {
        Some(base) => {
            let network = match (base.network.as_ref(), permissions.network.as_ref()) {
                (
                    Some(NetworkPermissions {
                        enabled: Some(true),
                    }),
                    _,
                )
                | (
                    _,
                    Some(NetworkPermissions {
                        enabled: Some(true),
                    }),
                ) => Some(NetworkPermissions {
                    enabled: Some(true),
                }),
                _ => None,
            };
            let file_system = match (base.file_system.as_ref(), permissions.file_system.as_ref()) {
                (Some(base), Some(permissions)) => Some(FileSystemPermissions {
                    read: merge_permission_paths(base.read.as_ref(), permissions.read.as_ref()),
                    write: merge_permission_paths(base.write.as_ref(), permissions.write.as_ref()),
                })
                .filter(|file_system| !file_system.is_empty()),
                (Some(base), None) => Some(base.clone()),
                (None, Some(permissions)) => Some(permissions.clone()),
                (None, None) => None,
            };

            Some(PermissionProfile {
                network,
                file_system,
            })
            .filter(|permissions| !permissions.is_empty())
        }
        None => Some(permissions.clone()).filter(|permissions| !permissions.is_empty()),
    }
}

fn merge_permission_paths<T>(base: Option<&Vec<T>>, permissions: Option<&Vec<T>>) -> Option<Vec<T>>
where
    T: Clone + Eq + Hash,
{
    match (base, permissions) {
        (Some(base), Some(permissions)) => {
            let mut merged = Vec::with_capacity(base.len() + permissions.len());
            let mut seen = HashSet::with_capacity(base.len() + permissions.len());

            for path in base.iter().chain(permissions.iter()) {
                if seen.insert(path.clone()) {
                    merged.push(path.clone());
                }
            }

            Some(merged).filter(|paths| !paths.is_empty())
        }
        (Some(base), None) => Some(base.clone()),
        (None, Some(permissions)) => Some(permissions.clone()),
        (None, None) => None,
    }
}
