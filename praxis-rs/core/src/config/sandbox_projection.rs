use std::io;
use std::path::Path;

use praxis_config::Constrained;
use praxis_config::types::SandboxWorkspaceWrite;
use praxis_protocol::config_types::SandboxMode;
use praxis_protocol::config_types::WindowsSandboxLevel;
use praxis_protocol::permissions::FileSystemSandboxPolicy;
use praxis_protocol::permissions::NetworkSandboxPolicy;
use praxis_protocol::protocol::ReadOnlyAccess;
use praxis_protocol::protocol::SandboxPolicy;

use super::ConfigToml;

pub fn sandbox_policy_from_mode(mode: SandboxMode) -> SandboxPolicy {
    match mode {
        SandboxMode::ReadOnly => SandboxPolicy::new_read_only_policy(),
        SandboxMode::WorkspaceWrite => SandboxPolicy::new_workspace_write_policy(),
        SandboxMode::DangerFullAccess => SandboxPolicy::DangerFullAccess,
    }
}

pub fn file_system_policy_from_sandbox_policy(
    policy: &SandboxPolicy,
    cwd: &Path,
) -> FileSystemSandboxPolicy {
    FileSystemSandboxPolicy::from_sandbox_policy(policy, cwd)
}

pub fn network_policy_from_sandbox_policy(policy: &SandboxPolicy) -> NetworkSandboxPolicy {
    NetworkSandboxPolicy::from(policy)
}

pub fn split_sandbox_policy(
    policy: &SandboxPolicy,
    cwd: &Path,
) -> (FileSystemSandboxPolicy, NetworkSandboxPolicy) {
    (
        file_system_policy_from_sandbox_policy(policy, cwd),
        network_policy_from_sandbox_policy(policy),
    )
}

pub fn sandbox_policy_from_split(
    file_system: &FileSystemSandboxPolicy,
    network: NetworkSandboxPolicy,
    cwd: &Path,
) -> io::Result<SandboxPolicy> {
    file_system.to_sandbox_policy(network, cwd)
}

pub(crate) fn derive_sandbox_policy(
    config: &ConfigToml,
    sandbox_mode_override: Option<SandboxMode>,
    profile_sandbox_mode: Option<SandboxMode>,
    windows_sandbox_level: WindowsSandboxLevel,
    resolved_cwd: &Path,
    sandbox_policy_constraint: Option<&Constrained<SandboxPolicy>>,
) -> SandboxPolicy {
    let sandbox_mode_was_explicit = sandbox_mode_override.is_some()
        || profile_sandbox_mode.is_some()
        || config.sandbox_mode.is_some();
    let resolved_sandbox_mode = sandbox_mode_override
        .or(profile_sandbox_mode)
        .or(config.sandbox_mode)
        .or_else(|| {
            config.get_active_project(resolved_cwd).and_then(|p| {
                if p.is_trusted() || p.is_untrusted() {
                    if cfg!(target_os = "windows")
                        && windows_sandbox_level == WindowsSandboxLevel::Disabled
                    {
                        Some(SandboxMode::ReadOnly)
                    } else {
                        Some(SandboxMode::WorkspaceWrite)
                    }
                } else {
                    None
                }
            })
        })
        .unwrap_or_default();
    let mut sandbox_policy = match resolved_sandbox_mode {
        SandboxMode::ReadOnly => SandboxPolicy::new_read_only_policy(),
        SandboxMode::WorkspaceWrite => match config.sandbox_workspace_write.as_ref() {
            Some(SandboxWorkspaceWrite {
                writable_roots,
                network_access,
                exclude_tmpdir_env_var,
                exclude_slash_tmp,
            }) => SandboxPolicy::WorkspaceWrite {
                writable_roots: writable_roots.clone(),
                read_only_access: ReadOnlyAccess::FullAccess,
                network_access: *network_access,
                exclude_tmpdir_env_var: *exclude_tmpdir_env_var,
                exclude_slash_tmp: *exclude_slash_tmp,
            },
            None => SandboxPolicy::new_workspace_write_policy(),
        },
        SandboxMode::DangerFullAccess => SandboxPolicy::DangerFullAccess,
    };
    let downgrade_workspace_write_if_unsupported = |policy: &mut SandboxPolicy| {
        if cfg!(target_os = "windows")
            && windows_sandbox_level == WindowsSandboxLevel::Disabled
            && matches!(&*policy, SandboxPolicy::WorkspaceWrite { .. })
        {
            *policy = SandboxPolicy::new_read_only_policy();
        }
    };
    if matches!(resolved_sandbox_mode, SandboxMode::WorkspaceWrite) {
        downgrade_workspace_write_if_unsupported(&mut sandbox_policy);
    }
    if !sandbox_mode_was_explicit
        && let Some(constraint) = sandbox_policy_constraint
        && let Err(err) = constraint.can_set(&sandbox_policy)
    {
        tracing::warn!(
            error = %err,
            "default sandbox policy is disallowed by requirements; falling back to required default"
        );
        sandbox_policy = constraint.get().clone();
        downgrade_workspace_write_if_unsupported(&mut sandbox_policy);
    }
    sandbox_policy
}
