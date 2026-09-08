use super::*;
use pretty_assertions::assert_eq;

fn unsupported_windows_restricted_token_sandbox_reason(
    sandbox: SandboxType,
    sandbox_policy: &SandboxPolicy,
    file_system_sandbox_policy: &FileSystemSandboxPolicy,
    network_sandbox_policy: NetworkSandboxPolicy,
    sandbox_policy_cwd: &Path,
    windows_sandbox_level: WindowsSandboxLevel,
) -> Option<String> {
    resolve_windows_restricted_token_filesystem_overlay(
        sandbox,
        sandbox_policy,
        file_system_sandbox_policy,
        network_sandbox_policy,
        sandbox_policy_cwd,
        windows_sandbox_level,
    )
    .err()
}

#[test]
fn windows_restricted_token_skips_external_sandbox_policies() {
    let policy = SandboxPolicy::ExternalSandbox {
        network_access: praxis_protocol::protocol::NetworkAccess::Restricted,
    };
    let file_system_policy = FileSystemSandboxPolicy::from(&policy);

    assert_eq!(
        should_use_windows_restricted_token_sandbox(
            SandboxType::WindowsRestrictedToken,
            &policy,
            &file_system_policy,
        ),
        false
    );
}

#[test]
fn windows_restricted_token_rejects_network_only_restrictions() {
    let policy = SandboxPolicy::ExternalSandbox {
        network_access: praxis_protocol::protocol::NetworkAccess::Restricted,
    };
    let file_system_policy = FileSystemSandboxPolicy::unrestricted();
    let sandbox_policy_cwd = std::env::current_dir().expect("cwd");

    assert_eq!(
            unsupported_windows_restricted_token_sandbox_reason(
                SandboxType::WindowsRestrictedToken,
                &policy,
                &file_system_policy,
                NetworkSandboxPolicy::Restricted,
                &sandbox_policy_cwd,
                WindowsSandboxLevel::RestrictedToken,
            ),
            Some(
                "windows sandbox backend cannot enforce file_system=Unrestricted, network=Restricted, protocol_policy=ExternalSandbox { network_access: Restricted }; refusing to run unsandboxed".to_string()
            )
        );
}

#[test]
fn windows_restricted_token_allows_workspace_write_projection_policies() {
    let policy = SandboxPolicy::WorkspaceWrite {
        writable_roots: vec![],
        read_only_access: praxis_protocol::protocol::ReadOnlyAccess::FullAccess,
        network_access: false,
        exclude_tmpdir_env_var: true,
        exclude_slash_tmp: true,
    };
    let file_system_policy = FileSystemSandboxPolicy::from(&policy);
    let sandbox_policy_cwd = std::env::current_dir().expect("cwd");

    assert_eq!(
        unsupported_windows_restricted_token_sandbox_reason(
            SandboxType::WindowsRestrictedToken,
            &policy,
            &file_system_policy,
            NetworkSandboxPolicy::Restricted,
            &sandbox_policy_cwd,
            WindowsSandboxLevel::RestrictedToken,
        ),
        None
    );
}

#[test]
fn windows_restricted_token_rejects_split_only_filesystem_policies() {
    let temp_dir = tempfile::TempDir::new().expect("tempdir");
    let docs = temp_dir.path().join("docs");
    std::fs::create_dir_all(&docs).expect("create docs");
    let policy = SandboxPolicy::WorkspaceWrite {
        writable_roots: vec![],
        read_only_access: praxis_protocol::protocol::ReadOnlyAccess::FullAccess,
        network_access: false,
        exclude_tmpdir_env_var: true,
        exclude_slash_tmp: true,
    };
    let file_system_policy = FileSystemSandboxPolicy::restricted(vec![
        praxis_protocol::permissions::FileSystemSandboxEntry {
            path: praxis_protocol::permissions::FileSystemPath::Special {
                value: praxis_protocol::permissions::FileSystemSpecialPath::CurrentWorkingDirectory,
            },
            access: praxis_protocol::permissions::FileSystemAccessMode::Write,
        },
        praxis_protocol::permissions::FileSystemSandboxEntry {
            path: praxis_protocol::permissions::FileSystemPath::Path {
                path: praxis_utils_absolute_path::AbsolutePathBuf::from_absolute_path(&docs)
                    .expect("absolute docs"),
            },
            access: praxis_protocol::permissions::FileSystemAccessMode::Read,
        },
    ]);

    assert_eq!(
        unsupported_windows_restricted_token_sandbox_reason(
            SandboxType::WindowsRestrictedToken,
            &policy,
            &file_system_policy,
            NetworkSandboxPolicy::Restricted,
            temp_dir.path(),
            WindowsSandboxLevel::RestrictedToken,
        ),
        Some(
            "windows unelevated restricted-token sandbox cannot enforce split filesystem read restrictions directly; refusing to run unsandboxed"
                .to_string()
        )
    );
}

#[test]
fn windows_restricted_token_rejects_root_write_read_only_carveouts() {
    let temp_dir = tempfile::TempDir::new().expect("tempdir");
    let docs = temp_dir.path().join("docs");
    std::fs::create_dir_all(&docs).expect("create docs");
    let policy = SandboxPolicy::WorkspaceWrite {
        writable_roots: vec![],
        read_only_access: praxis_protocol::protocol::ReadOnlyAccess::FullAccess,
        network_access: false,
        exclude_tmpdir_env_var: true,
        exclude_slash_tmp: true,
    };
    let file_system_policy = FileSystemSandboxPolicy::restricted(vec![
        praxis_protocol::permissions::FileSystemSandboxEntry {
            path: praxis_protocol::permissions::FileSystemPath::Special {
                value: praxis_protocol::permissions::FileSystemSpecialPath::Root,
            },
            access: praxis_protocol::permissions::FileSystemAccessMode::Write,
        },
        praxis_protocol::permissions::FileSystemSandboxEntry {
            path: praxis_protocol::permissions::FileSystemPath::Path {
                path: praxis_utils_absolute_path::AbsolutePathBuf::from_absolute_path(&docs)
                    .expect("absolute docs"),
            },
            access: praxis_protocol::permissions::FileSystemAccessMode::Read,
        },
    ]);

    assert_eq!(
        unsupported_windows_restricted_token_sandbox_reason(
            SandboxType::WindowsRestrictedToken,
            &policy,
            &file_system_policy,
            NetworkSandboxPolicy::Restricted,
            temp_dir.path(),
            WindowsSandboxLevel::RestrictedToken,
        ),
        Some(
            "windows unelevated restricted-token sandbox cannot enforce split writable root sets directly; refusing to run unsandboxed"
                .to_string()
        )
    );
}

#[test]
fn windows_restricted_token_supports_full_read_split_write_read_carveouts() {
    let temp_dir = tempfile::TempDir::new().expect("tempdir");
    let cwd = dunce::canonicalize(temp_dir.path()).expect("canonicalize temp dir");
    let docs = cwd.join("docs");
    std::fs::create_dir_all(&docs).expect("create docs");
    let policy = SandboxPolicy::WorkspaceWrite {
        writable_roots: vec![],
        read_only_access: praxis_protocol::protocol::ReadOnlyAccess::FullAccess,
        network_access: false,
        exclude_tmpdir_env_var: true,
        exclude_slash_tmp: true,
    };
    let file_system_policy = FileSystemSandboxPolicy::restricted(vec![
        praxis_protocol::permissions::FileSystemSandboxEntry {
            path: praxis_protocol::permissions::FileSystemPath::Special {
                value: praxis_protocol::permissions::FileSystemSpecialPath::Root,
            },
            access: praxis_protocol::permissions::FileSystemAccessMode::Read,
        },
        praxis_protocol::permissions::FileSystemSandboxEntry {
            path: praxis_protocol::permissions::FileSystemPath::Special {
                value: praxis_protocol::permissions::FileSystemSpecialPath::CurrentWorkingDirectory,
            },
            access: praxis_protocol::permissions::FileSystemAccessMode::Write,
        },
        praxis_protocol::permissions::FileSystemSandboxEntry {
            path: praxis_protocol::permissions::FileSystemPath::Path {
                path: praxis_utils_absolute_path::AbsolutePathBuf::from_absolute_path(&docs)
                    .expect("absolute docs"),
            },
            access: praxis_protocol::permissions::FileSystemAccessMode::Read,
        },
    ]);

    #[cfg(windows)]
    let expected_deny_write_paths = vec![
        praxis_utils_absolute_path::AbsolutePathBuf::from_absolute_path(cwd.join(".praxis"))
            .expect("absolute .codex"),
        praxis_utils_absolute_path::AbsolutePathBuf::from_absolute_path(&docs)
            .expect("absolute docs"),
    ];
    #[cfg(not(windows))]
    let expected_deny_write_paths = vec![
        praxis_utils_absolute_path::AbsolutePathBuf::from_absolute_path(&docs)
            .expect("absolute docs"),
    ];

    assert_eq!(
        resolve_windows_restricted_token_filesystem_overlay(
            SandboxType::WindowsRestrictedToken,
            &policy,
            &file_system_policy,
            NetworkSandboxPolicy::Restricted,
            &cwd,
            WindowsSandboxLevel::RestrictedToken,
        ),
        Ok(Some(WindowsRestrictedTokenFilesystemOverlay {
            additional_deny_write_paths: expected_deny_write_paths,
        }))
    );
}

#[test]
fn windows_elevated_rejects_split_write_read_carveouts() {
    let temp_dir = tempfile::TempDir::new().expect("tempdir");
    let docs = temp_dir.path().join("docs");
    std::fs::create_dir_all(&docs).expect("create docs");
    let policy = SandboxPolicy::WorkspaceWrite {
        writable_roots: vec![],
        read_only_access: praxis_protocol::protocol::ReadOnlyAccess::FullAccess,
        network_access: false,
        exclude_tmpdir_env_var: false,
        exclude_slash_tmp: false,
    };
    let file_system_policy = FileSystemSandboxPolicy::restricted(vec![
        praxis_protocol::permissions::FileSystemSandboxEntry {
            path: praxis_protocol::permissions::FileSystemPath::Special {
                value: praxis_protocol::permissions::FileSystemSpecialPath::Root,
            },
            access: praxis_protocol::permissions::FileSystemAccessMode::Read,
        },
        praxis_protocol::permissions::FileSystemSandboxEntry {
            path: praxis_protocol::permissions::FileSystemPath::Special {
                value: praxis_protocol::permissions::FileSystemSpecialPath::CurrentWorkingDirectory,
            },
            access: praxis_protocol::permissions::FileSystemAccessMode::Write,
        },
        praxis_protocol::permissions::FileSystemSandboxEntry {
            path: praxis_protocol::permissions::FileSystemPath::Path {
                path: praxis_utils_absolute_path::AbsolutePathBuf::from_absolute_path(&docs)
                    .expect("absolute docs"),
            },
            access: praxis_protocol::permissions::FileSystemAccessMode::Read,
        },
    ]);

    assert_eq!(
        unsupported_windows_restricted_token_sandbox_reason(
            SandboxType::WindowsRestrictedToken,
            &policy,
            &file_system_policy,
            NetworkSandboxPolicy::Restricted,
            temp_dir.path(),
            WindowsSandboxLevel::Elevated,
        ),
        Some(
            "windows elevated sandbox backend cannot enforce split filesystem permissions directly; refusing to run unsandboxed"
                .to_string()
        )
    );
}
