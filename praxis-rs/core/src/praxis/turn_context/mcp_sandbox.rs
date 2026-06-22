use praxis_mcp::mcp_connection_manager::SandboxState;
use tracing::warn;

use crate::config::Config;

use super::super::Session;
use super::super::SessionConfiguration;

pub(super) async fn sync_for_turn(
    session: &Session,
    session_configuration: &SessionConfiguration,
    per_turn_config: &Config,
    sandbox_policy_changed: bool,
) {
    session
        .services
        .mcp_connection_manager
        .read()
        .await
        .set_approval_policy(&session_configuration.approval_policy);

    if !sandbox_policy_changed {
        return;
    }

    let sandbox_state = SandboxState {
        sandbox_policy: per_turn_config.permissions.sandbox_policy.get().clone(),
        praxis_linux_sandbox_exe: per_turn_config.praxis_linux_sandbox_exe.clone(),
        sandbox_cwd: per_turn_config.cwd.to_path_buf(),
        use_legacy_landlock: per_turn_config.features.use_legacy_landlock(),
    };
    if let Err(e) = session
        .services
        .mcp_connection_manager
        .read()
        .await
        .notify_sandbox_state_change(&sandbox_state)
        .await
    {
        warn!("Failed to notify sandbox state change to MCP servers: {e:#}");
    }
}
