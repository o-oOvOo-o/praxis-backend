use std::path::Path;

use praxis_protocol::permissions::FileSystemSandboxPolicy;
use praxis_protocol::permissions::NetworkSandboxPolicy;
use praxis_protocol::protocol::SandboxPolicy;

pub(crate) fn split_sandbox_policy(
    policy: &SandboxPolicy,
    cwd: &Path,
) -> (FileSystemSandboxPolicy, NetworkSandboxPolicy) {
    (
        FileSystemSandboxPolicy::from_sandbox_policy(policy, cwd),
        NetworkSandboxPolicy::from(policy),
    )
}
