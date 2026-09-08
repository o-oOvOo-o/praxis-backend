use crate::SandboxType;
use praxis_protocol::config_types::WindowsSandboxLevel;
use praxis_protocol::permissions::FileSystemSandboxKind;
use praxis_protocol::permissions::FileSystemSandboxPolicy;
use praxis_protocol::permissions::NetworkSandboxPolicy;
use praxis_protocol::protocol::SandboxPolicy;
use praxis_utils_absolute_path::AbsolutePathBuf;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// Additional deny-write carveouts enforced by the Windows restricted-token backend.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowsRestrictedTokenFilesystemOverlay {
    pub additional_deny_write_paths: Vec<AbsolutePathBuf>,
}

fn should_use_windows_restricted_token_sandbox(
    sandbox: SandboxType,
    sandbox_policy: &SandboxPolicy,
    file_system_sandbox_policy: &FileSystemSandboxPolicy,
) -> bool {
    sandbox == SandboxType::WindowsRestrictedToken
        && file_system_sandbox_policy.kind == FileSystemSandboxKind::Restricted
        && !matches!(
            sandbox_policy,
            SandboxPolicy::DangerFullAccess | SandboxPolicy::ExternalSandbox { .. }
        )
}

pub fn resolve_windows_restricted_token_filesystem_overlay(
    sandbox: SandboxType,
    sandbox_policy: &SandboxPolicy,
    file_system_sandbox_policy: &FileSystemSandboxPolicy,
    network_sandbox_policy: NetworkSandboxPolicy,
    sandbox_policy_cwd: &Path,
    windows_sandbox_level: WindowsSandboxLevel,
) -> std::result::Result<Option<WindowsRestrictedTokenFilesystemOverlay>, String> {
    if sandbox != SandboxType::WindowsRestrictedToken {
        return Ok(None);
    }

    let needs_direct_runtime_enforcement = file_system_sandbox_policy
        .needs_direct_runtime_enforcement(network_sandbox_policy, sandbox_policy_cwd);

    if should_use_windows_restricted_token_sandbox(
        sandbox,
        sandbox_policy,
        file_system_sandbox_policy,
    ) && !needs_direct_runtime_enforcement
    {
        return Ok(None);
    }

    if !should_use_windows_restricted_token_sandbox(
        sandbox,
        sandbox_policy,
        file_system_sandbox_policy,
    ) {
        return Err(format!(
            "windows sandbox backend cannot enforce file_system={:?}, network={network_sandbox_policy:?}, protocol_policy={sandbox_policy:?}; refusing to run unsandboxed",
            file_system_sandbox_policy.kind,
        ));
    }

    if windows_sandbox_level != WindowsSandboxLevel::RestrictedToken {
        return Err(
            "windows elevated sandbox backend cannot enforce split filesystem permissions directly; refusing to run unsandboxed"
                .to_string(),
        );
    }

    if !file_system_sandbox_policy.has_full_disk_read_access() {
        return Err(
            "windows unelevated restricted-token sandbox cannot enforce split filesystem read restrictions directly; refusing to run unsandboxed"
                .to_string(),
        );
    }

    if !file_system_sandbox_policy
        .get_unreadable_roots_with_cwd(sandbox_policy_cwd)
        .is_empty()
    {
        return Err(
            "windows unelevated restricted-token sandbox cannot enforce unreadable split filesystem carveouts directly; refusing to run unsandboxed"
                .to_string(),
        );
    }

    let legacy_writable_roots = sandbox_policy.get_writable_roots_with_cwd(sandbox_policy_cwd);
    let split_writable_roots =
        file_system_sandbox_policy.get_writable_roots_with_cwd(sandbox_policy_cwd);
    let legacy_root_paths: BTreeSet<PathBuf> = legacy_writable_roots
        .iter()
        .map(|root| normalize_windows_overlay_path(root.root.as_path()))
        .collect::<std::result::Result<_, _>>()?;
    let split_root_paths: BTreeSet<PathBuf> = split_writable_roots
        .iter()
        .map(|root| normalize_windows_overlay_path(root.root.as_path()))
        .collect::<std::result::Result<_, _>>()?;

    if legacy_root_paths != split_root_paths {
        return Err(
            "windows unelevated restricted-token sandbox cannot enforce split writable root sets directly; refusing to run unsandboxed"
                .to_string(),
        );
    }

    for writable_root in &split_writable_roots {
        for read_only_subpath in &writable_root.read_only_subpaths {
            if split_writable_roots.iter().any(|candidate| {
                candidate.root.as_path() != writable_root.root.as_path()
                    && candidate
                        .root
                        .as_path()
                        .starts_with(read_only_subpath.as_path())
            }) {
                return Err(
                    "windows unelevated restricted-token sandbox cannot reopen writable descendants under read-only carveouts directly; refusing to run unsandboxed"
                        .to_string(),
                );
            }
        }
    }

    let mut additional_deny_write_paths = BTreeSet::new();
    for split_root in &split_writable_roots {
        let split_root_path = normalize_windows_overlay_path(split_root.root.as_path())?;
        let Some(legacy_root) = legacy_writable_roots.iter().find(|candidate| {
            normalize_windows_overlay_path(candidate.root.as_path())
                .is_ok_and(|candidate_path| candidate_path == split_root_path)
        }) else {
            return Err(
                "windows unelevated restricted-token sandbox cannot enforce split writable root sets directly; refusing to run unsandboxed"
                    .to_string(),
            );
        };

        for read_only_subpath in &split_root.read_only_subpaths {
            if !legacy_root
                .read_only_subpaths
                .iter()
                .any(|candidate| candidate == read_only_subpath)
            {
                additional_deny_write_paths
                    .insert(normalize_windows_overlay_path(read_only_subpath.as_path())?);
            }
        }
    }

    if additional_deny_write_paths.is_empty() {
        return Ok(None);
    }

    Ok(Some(WindowsRestrictedTokenFilesystemOverlay {
        additional_deny_write_paths: additional_deny_write_paths
            .into_iter()
            .map(|path| AbsolutePathBuf::from_absolute_path(path).map_err(|err| err.to_string()))
            .collect::<std::result::Result<_, _>>()?,
    }))
}

fn normalize_windows_overlay_path(path: &Path) -> std::result::Result<PathBuf, String> {
    AbsolutePathBuf::from_absolute_path(dunce::simplified(path))
        .map(AbsolutePathBuf::into_path_buf)
        .map_err(|err| err.to_string())
}

#[cfg(test)]
#[path = "windows_policy_tests.rs"]
mod tests;
