use std::fmt::Debug;

use praxis_protocol::config_types::WindowsSandboxLevel;
use praxis_protocol::permissions::FileSystemSandboxPolicy;
use praxis_protocol::permissions::NetworkSandboxPolicy;
use praxis_protocol::protocol::AskForApproval;
use praxis_protocol::protocol::SandboxPolicy;
use tokio::sync::watch;

use crate::config::Constrained;

use super::super::SessionConfiguration;

#[derive(Clone, Debug)]
pub(crate) struct EffectivePermissions {
    pub(crate) approval_policy: Constrained<AskForApproval>,
    pub(crate) sandbox_policy: Constrained<SandboxPolicy>,
    pub(crate) file_system_sandbox_policy: FileSystemSandboxPolicy,
    pub(crate) network_sandbox_policy: NetworkSandboxPolicy,
    pub(crate) windows_sandbox_level: WindowsSandboxLevel,
}

impl EffectivePermissions {
    pub(crate) fn from_session_configuration(session_configuration: &SessionConfiguration) -> Self {
        Self {
            approval_policy: session_configuration.approval_policy.clone(),
            sandbox_policy: session_configuration.sandbox_policy.clone(),
            file_system_sandbox_policy: session_configuration.file_system_sandbox_policy.clone(),
            network_sandbox_policy: session_configuration.network_sandbox_policy,
            windows_sandbox_level: session_configuration.windows_sandbox_level,
        }
    }
}

#[derive(Clone)]
pub(crate) struct LiveEffectivePermissions {
    rx: watch::Receiver<EffectivePermissions>,
}

impl LiveEffectivePermissions {
    pub(crate) fn new(rx: watch::Receiver<EffectivePermissions>) -> Self {
        Self { rx }
    }

    pub(crate) fn snapshot(&self) -> EffectivePermissions {
        self.rx.borrow().clone()
    }
}

impl Debug for LiveEffectivePermissions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LiveEffectivePermissions")
            .field("snapshot", &self.snapshot())
            .finish()
    }
}
