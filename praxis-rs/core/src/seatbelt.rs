#![cfg(target_os = "macos")]

use crate::spawn::PRAXIS_SANDBOX_ENV_VAR;
use crate::spawn::SpawnChildRequest;
use crate::spawn::StdioPolicy;
use crate::spawn::spawn_child_async;
use praxis_network_proxy::NetworkProxy;
use praxis_protocol::permissions::FileSystemSandboxPolicy;
use praxis_protocol::permissions::NetworkSandboxPolicy;
use praxis_protocol::protocol::SandboxPolicy;
use praxis_sandboxing::seatbelt::MACOS_PATH_TO_SEATBELT_EXECUTABLE;
use praxis_sandboxing::seatbelt::create_seatbelt_command_args_for_policies;
use std::collections::HashMap;
use std::path::Path;
use std::path::PathBuf;
use tokio::process::Child;

pub async fn spawn_command_under_seatbelt(
    command: Vec<String>,
    command_cwd: PathBuf,
    sandbox_policy: &SandboxPolicy,
    sandbox_policy_cwd: &Path,
    stdio_policy: StdioPolicy,
    network: Option<&NetworkProxy>,
    mut env: HashMap<String, String>,
) -> std::io::Result<Child> {
    let args = create_seatbelt_command_args_for_policies(
        command,
        &FileSystemSandboxPolicy::from_sandbox_policy(sandbox_policy, sandbox_policy_cwd),
        NetworkSandboxPolicy::from(sandbox_policy),
        sandbox_policy_cwd,
        /*enforce_managed_network*/ false,
        network,
    );
    let arg0 = None;
    env.insert(PRAXIS_SANDBOX_ENV_VAR.to_string(), "seatbelt".to_string());
    spawn_child_async(SpawnChildRequest {
        program: PathBuf::from(MACOS_PATH_TO_SEATBELT_EXECUTABLE),
        args,
        arg0,
        cwd: command_cwd,
        network_sandbox_policy: NetworkSandboxPolicy::from(sandbox_policy),
        network,
        stdio_policy,
        env,
    })
    .await
}
