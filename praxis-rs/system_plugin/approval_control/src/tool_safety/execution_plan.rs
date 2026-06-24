use praxis_protocol::config_types::WindowsSandboxLevel;
use praxis_protocol::permissions::FileSystemSandboxPolicy;
use praxis_protocol::permissions::NetworkSandboxPolicy;
use praxis_protocol::protocol::SandboxPolicy;
use serde::Deserialize;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SandboxExecutionMode {
    NoSandbox,
    PraxisSandbox,
    ExternalSandbox,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SandboxExecutionPlan {
    pub mode: SandboxExecutionMode,
    pub sandbox_policy: SandboxPolicy,
    pub file_system_sandbox_policy: FileSystemSandboxPolicy,
    pub network_sandbox_policy: NetworkSandboxPolicy,
    pub windows_sandbox_level: WindowsSandboxLevel,
}

impl SandboxExecutionPlan {
    pub fn from_permissions(
        sandbox_policy: SandboxPolicy,
        file_system_sandbox_policy: FileSystemSandboxPolicy,
        network_sandbox_policy: NetworkSandboxPolicy,
        windows_sandbox_level: WindowsSandboxLevel,
    ) -> Self {
        let mode = match &sandbox_policy {
            SandboxPolicy::DangerFullAccess => SandboxExecutionMode::NoSandbox,
            SandboxPolicy::ExternalSandbox { .. } => SandboxExecutionMode::ExternalSandbox,
            SandboxPolicy::ReadOnly { .. } | SandboxPolicy::WorkspaceWrite { .. } => {
                SandboxExecutionMode::PraxisSandbox
            }
        };
        Self {
            mode,
            sandbox_policy,
            file_system_sandbox_policy,
            network_sandbox_policy,
            windows_sandbox_level,
        }
    }

    pub fn runs_without_sandbox(&self) -> bool {
        matches!(self.mode, SandboxExecutionMode::NoSandbox)
    }
}
