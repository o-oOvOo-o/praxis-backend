use praxis_mcp::mcp_connection_manager::SandboxState;

use crate::config::Config;
use crate::praxis::SessionConfiguration;

pub(super) fn build(config: &Config, session_configuration: &SessionConfiguration) -> SandboxState {
    SandboxState {
        sandbox_policy: session_configuration.sandbox_policy.get().clone(),
        praxis_linux_sandbox_exe: config.praxis_linux_sandbox_exe.clone(),
        sandbox_cwd: session_configuration.cwd.to_path_buf(),
        use_legacy_landlock: config.features.use_legacy_landlock(),
    }
}
