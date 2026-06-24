use praxis_protocol::config_types::ApprovalsReviewer;
use praxis_protocol::config_types::WindowsSandboxLevel;
use praxis_protocol::permissions::FileSystemSandboxKind;
use praxis_protocol::permissions::FileSystemSandboxPolicy;
use praxis_protocol::permissions::NetworkSandboxPolicy;
use praxis_protocol::protocol::AskForApproval;
use praxis_protocol::protocol::SandboxPolicy;
use serde::Deserialize;
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedTurnPermissions {
    pub approval_policy: AskForApproval,
    pub approvals_reviewer: ApprovalsReviewer,
    pub sandbox_policy: SandboxPolicy,
    pub file_system_sandbox_policy: FileSystemSandboxPolicy,
    pub network_sandbox_policy: NetworkSandboxPolicy,
    pub windows_sandbox_level: WindowsSandboxLevel,
    pub generation: u64,
}

impl ResolvedTurnPermissions {
    pub fn normalized(mut self) -> Self {
        if matches!(&self.sandbox_policy, SandboxPolicy::DangerFullAccess) {
            self.approval_policy = AskForApproval::Never;
            self.file_system_sandbox_policy = FileSystemSandboxPolicy::unrestricted();
            self.network_sandbox_policy = NetworkSandboxPolicy::Enabled;
            self.windows_sandbox_level = WindowsSandboxLevel::Disabled;
        }
        self
    }

    pub fn is_promptless_full_access(&self) -> bool {
        self.approval_policy == AskForApproval::Never
            && matches!(&self.sandbox_policy, SandboxPolicy::DangerFullAccess)
            && self.file_system_sandbox_policy.kind == FileSystemSandboxKind::Unrestricted
            && self.network_sandbox_policy == NetworkSandboxPolicy::Enabled
            && self.windows_sandbox_level == WindowsSandboxLevel::Disabled
    }
}
