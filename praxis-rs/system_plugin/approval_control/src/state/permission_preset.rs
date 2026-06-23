use praxis_protocol::protocol::AskForApproval;
use praxis_protocol::protocol::SandboxPolicy;
use serde::Deserialize;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionPreset {
    ReadOnly,
    WorkspaceWrite,
    FullAccess,
}

impl PermissionPreset {
    pub fn approval_policy(self) -> AskForApproval {
        match self {
            Self::ReadOnly | Self::WorkspaceWrite => AskForApproval::OnRequest,
            Self::FullAccess => AskForApproval::Never,
        }
    }

    pub fn sandbox_policy(self) -> SandboxPolicy {
        match self {
            Self::ReadOnly => SandboxPolicy::new_read_only_policy(),
            Self::WorkspaceWrite => SandboxPolicy::new_workspace_write_policy(),
            Self::FullAccess => SandboxPolicy::DangerFullAccess,
        }
    }
}

impl Default for PermissionPreset {
    fn default() -> Self {
        Self::WorkspaceWrite
    }
}
