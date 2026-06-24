use super::ResolvedTurnPermissions;
use praxis_protocol::config_types::ApprovalsReviewer;
use praxis_protocol::config_types::WindowsSandboxLevel;
use praxis_protocol::permissions::FileSystemSandboxPolicy;
use praxis_protocol::permissions::NetworkSandboxPolicy;
use praxis_protocol::protocol::AskForApproval;
use praxis_protocol::protocol::SandboxPolicy;
use serde::Deserialize;
use serde::Serialize;

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
    }
}
