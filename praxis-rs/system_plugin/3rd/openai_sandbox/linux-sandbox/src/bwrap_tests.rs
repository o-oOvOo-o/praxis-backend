use super::*;
use praxis_protocol::protocol::FileSystemAccessMode;
use praxis_protocol::protocol::FileSystemPath;
use praxis_protocol::protocol::FileSystemSandboxEntry;
use praxis_protocol::protocol::FileSystemSandboxPolicy;
use praxis_protocol::protocol::FileSystemSpecialPath;
use praxis_protocol::protocol::ReadOnlyAccess;
use praxis_protocol::protocol::SandboxPolicy;
use praxis_utils_absolute_path::AbsolutePathBuf;
use pretty_assertions::assert_eq;
use tempfile::TempDir;

#[test]
fn full_disk_write_full_network_returns_unwrapped_command() {
    let command = vec!["/bin/true".to_string()];
    let args = create_bwrap_command_args(
        command.clone(),
        &FileSystemSandboxPolicy::from(&SandboxPolicy::DangerFullAccess),
        Path::new("/"),
        Path::new("/"),
        BwrapOptions {
            mount_proc: true,
            network_mode: BwrapNetworkMode::FullAccess,
        },
    )
    .expect("create bwrap args");

    assert_eq!(args.args, command);
}

#[test]
fn full_disk_write_proxy_only_keeps_full_filesystem_but_unshares_network() {
    let command = vec!["/bin/true".to_string()];
    let args = create_bwrap_command_args(
        command,
        &FileSystemSandboxPolicy::from(&SandboxPolicy::DangerFullAccess),
        Path::new("/"),
        Path::new("/"),
        BwrapOptions {
            mount_proc: true,
            network_mode: BwrapNetworkMode::ProxyOnly,
        },
    )
    .expect("create bwrap args");

    assert_eq!(
        args.args,
        vec![
            "--new-session".to_string(),
            "--die-with-parent".to_string(),
            "--bind".to_string(),
            "/".to_string(),
            "/".to_string(),
            "--unshare-user".to_string(),
            "--unshare-pid".to_string(),
            "--unshare-net".to_string(),
            "--proc".to_string(),
            "/proc".to_string(),
            "--".to_string(),
            "/bin/true".to_string(),
        ]
    );
}

#[cfg(unix)]
#[test]
fn restricted_policy_chdirs_to_canonical_command_cwd() {
    let temp_dir = TempDir::new().expect("temp dir");
    let real_root = temp_dir.path().join("real");
    let real_subdir = real_root.join("subdir");
    let link_root = temp_dir.path().join("link");
    std::fs::create_dir_all(&real_subdir).expect("create real subdir");
    std::os::unix::fs::symlink(&real_root, &link_root).expect("create symlinked root");

    let sandbox_policy_cwd = AbsolutePathBuf::from_absolute_path(&link_root)
        .expect("absolute symlinked root")
        .to_path_buf();
    let command_cwd = link_root.join("subdir");
    let canonical_command_cwd = real_subdir
        .canonicalize()
        .expect("canonicalize command cwd");
    let policy = FileSystemSandboxPolicy::restricted(vec![
        FileSystemSandboxEntry {
            path: FileSystemPath::Special {
                value: FileSystemSpecialPath::Minimal,
            },
            access: FileSystemAccessMode::Read,
        },
        FileSystemSandboxEntry {
            path: FileSystemPath::Special {
                value: FileSystemSpecialPath::CurrentWorkingDirectory,
            },
            access: FileSystemAccessMode::Write,
        },
    ]);

    let args = create_bwrap_command_args(
        vec!["/bin/true".to_string()],
        &policy,
        sandbox_policy_cwd.as_path(),
        &command_cwd,
        BwrapOptions::default(),
    )
    .expect("create bwrap args");
    let canonical_command_cwd = path_to_string(&canonical_command_cwd);
    let link_command_cwd = path_to_string(&command_cwd);

    assert!(
        args.args
            .windows(2)
            .any(|window| { window == ["--chdir", canonical_command_cwd.as_str()] })
    );
    assert!(
        !args
            .args
            .windows(2)
            .any(|window| { window == ["--chdir", link_command_cwd.as_str()] })
    );
}

#[test]
fn ignores_missing_writable_roots() {
    let temp_dir = TempDir::new().expect("temp dir");
    let existing_root = temp_dir.path().join("existing");
    let missing_root = temp_dir.path().join("missing");
    std::fs::create_dir(&existing_root).expect("create existing root");

    let policy = SandboxPolicy::WorkspaceWrite {
        writable_roots: vec![
            AbsolutePathBuf::try_from(existing_root.as_path()).expect("absolute existing root"),
            AbsolutePathBuf::try_from(missing_root.as_path()).expect("absolute missing root"),
        ],
        read_only_access: Default::default(),
        network_access: false,
        exclude_tmpdir_env_var: true,
        exclude_slash_tmp: true,
    };

    let args = create_filesystem_args(&FileSystemSandboxPolicy::from(&policy), temp_dir.path())
        .expect("filesystem args");
    let existing_root = path_to_string(&existing_root);
    let missing_root = path_to_string(&missing_root);

    assert!(
        args.args
            .windows(3)
            .any(|window| { window == ["--bind", existing_root.as_str(), existing_root.as_str()] }),
        "existing writable root should be rebound writable",
    );
    assert!(
        !args.args.iter().any(|arg| arg == &missing_root),
        "missing writable root should be skipped",
    );
}

#[test]
fn mounts_dev_before_writable_dev_binds() {
    let sandbox_policy = SandboxPolicy::WorkspaceWrite {
        writable_roots: vec![AbsolutePathBuf::try_from(Path::new("/dev")).expect("/dev path")],
        read_only_access: Default::default(),
        network_access: false,
        exclude_tmpdir_env_var: true,
        exclude_slash_tmp: true,
    };

    let args = create_filesystem_args(
        &FileSystemSandboxPolicy::from(&sandbox_policy),
        Path::new("/"),
    )
    .expect("bwrap fs args");
    assert_eq!(
        args.args,
        vec![
            // Start from a read-only view of the full filesystem.
            "--ro-bind".to_string(),
            "/".to_string(),
            "/".to_string(),
            // Recreate a writable /dev inside the sandbox.
            "--dev".to_string(),
            "/dev".to_string(),
            // Make the writable root itself writable again.
            "--bind".to_string(),
            "/".to_string(),
            "/".to_string(),
            // Mask the default protected .praxis subpath under that writable
            // root. Because the root is `/` in this test, the carveout path
            // appears as `/.praxis`.
            "--ro-bind".to_string(),
            "/dev/null".to_string(),
            "/.praxis".to_string(),
            // Rebind /dev after the root bind so device nodes remain
            // writable/usable inside the writable root.
            "--bind".to_string(),
            "/dev".to_string(),
            "/dev".to_string(),
        ]
    );
}

#[test]
fn restricted_read_only_uses_scoped_read_roots_instead_of_erroring() {
    let temp_dir = TempDir::new().expect("temp dir");
    let readable_root = temp_dir.path().join("readable");
    std::fs::create_dir(&readable_root).expect("create readable root");

    let policy = SandboxPolicy::ReadOnly {
        access: ReadOnlyAccess::Restricted {
            include_platform_defaults: false,
            readable_roots: vec![
                AbsolutePathBuf::try_from(readable_root.as_path()).expect("absolute readable root"),
            ],
        },
        network_access: false,
    };

    let args = create_filesystem_args(&FileSystemSandboxPolicy::from(&policy), temp_dir.path())
        .expect("filesystem args");

    assert_eq!(args.args[0..4], ["--tmpfs", "/", "--dev", "/dev"]);

    let readable_root_str = path_to_string(&readable_root);
    assert!(args.args.windows(3).any(|window| {
        window
            == [
                "--ro-bind",
                readable_root_str.as_str(),
                readable_root_str.as_str(),
            ]
    }));
}

#[test]
fn restricted_read_only_with_platform_defaults_includes_usr_when_present() {
    let temp_dir = TempDir::new().expect("temp dir");
    let policy = SandboxPolicy::ReadOnly {
        access: ReadOnlyAccess::Restricted {
            include_platform_defaults: true,
            readable_roots: Vec::new(),
        },
        network_access: false,
    };

    // `ReadOnlyAccess::Restricted` always includes `cwd` as a readable
    // root. Using `"/"` here would intentionally collapse to broad read
    // access, so use a non-root cwd to exercise the restricted path.
    let args = create_filesystem_args(&FileSystemSandboxPolicy::from(&policy), temp_dir.path())
        .expect("filesystem args");

    assert!(
        args.args
            .starts_with(&["--tmpfs".to_string(), "/".to_string()])
    );

    if Path::new("/usr").exists() {
        assert!(
            args.args
                .windows(3)
                .any(|window| window == ["--ro-bind", "/usr", "/usr"])
        );
    }
}

#[test]
fn split_policy_reapplies_unreadable_carveouts_after_writable_binds() {
    let temp_dir = TempDir::new().expect("temp dir");
    let writable_root = temp_dir.path().join("workspace");
    let blocked = writable_root.join("blocked");
    std::fs::create_dir_all(&blocked).expect("create blocked dir");
    let writable_root =
        AbsolutePathBuf::from_absolute_path(&writable_root).expect("absolute writable root");
    let blocked = AbsolutePathBuf::from_absolute_path(&blocked).expect("absolute blocked dir");
    let writable_root_str = path_to_string(writable_root.as_path());
    let blocked_str = path_to_string(blocked.as_path());
    let policy = FileSystemSandboxPolicy::restricted(vec![
        FileSystemSandboxEntry {
            path: FileSystemPath::Path {
                path: writable_root,
            },
            access: FileSystemAccessMode::Write,
        },
        FileSystemSandboxEntry {
            path: FileSystemPath::Path { path: blocked },
            access: FileSystemAccessMode::None,
        },
    ]);

    let args = create_filesystem_args(&policy, temp_dir.path()).expect("filesystem args");

    assert!(args.args.windows(3).any(|window| {
        window
            == [
                "--bind",
                writable_root_str.as_str(),
                writable_root_str.as_str(),
            ]
    }));
    let blocked_mask_index = args
        .args
        .windows(6)
        .position(|window| {
            window
                == [
                    "--perms",
                    "000",
                    "--tmpfs",
                    blocked_str.as_str(),
                    "--remount-ro",
                    blocked_str.as_str(),
                ]
        })
        .expect("blocked directory should be remounted unreadable");

    let writable_root_bind_index = args
        .args
        .windows(3)
        .position(|window| {
            window
                == [
                    "--bind",
                    writable_root_str.as_str(),
                    writable_root_str.as_str(),
                ]
        })
        .expect("writable root should be rebound writable");

    assert!(
        writable_root_bind_index < blocked_mask_index,
        "expected unreadable carveout to be re-applied after writable bind: {:#?}",
        args.args
    );
}

#[test]
fn split_policy_reenables_nested_writable_subpaths_after_read_only_parent() {
    let temp_dir = TempDir::new().expect("temp dir");
    let writable_root = temp_dir.path().join("workspace");
    let docs = writable_root.join("docs");
    let docs_public = docs.join("public");
    std::fs::create_dir_all(&docs_public).expect("create docs/public");
    let writable_root =
        AbsolutePathBuf::from_absolute_path(&writable_root).expect("absolute writable root");
    let docs = AbsolutePathBuf::from_absolute_path(&docs).expect("absolute docs");
    let docs_public =
        AbsolutePathBuf::from_absolute_path(&docs_public).expect("absolute docs/public");
    let policy = FileSystemSandboxPolicy::restricted(vec![
        FileSystemSandboxEntry {
            path: FileSystemPath::Path {
                path: writable_root,
            },
            access: FileSystemAccessMode::Write,
        },
        FileSystemSandboxEntry {
            path: FileSystemPath::Path { path: docs.clone() },
            access: FileSystemAccessMode::Read,
        },
        FileSystemSandboxEntry {
            path: FileSystemPath::Path {
                path: docs_public.clone(),
            },
            access: FileSystemAccessMode::Write,
        },
    ]);

    let args = create_filesystem_args(&policy, temp_dir.path()).expect("filesystem args");
    let docs_str = path_to_string(docs.as_path());
    let docs_public_str = path_to_string(docs_public.as_path());
    let docs_ro_index = args
        .args
        .windows(3)
        .position(|window| window == ["--ro-bind", docs_str.as_str(), docs_str.as_str()])
        .expect("docs should be remounted read-only");
    let docs_public_rw_index = args
        .args
        .windows(3)
        .position(|window| window == ["--bind", docs_public_str.as_str(), docs_public_str.as_str()])
        .expect("docs/public should be rebound writable");

    assert!(
        docs_ro_index < docs_public_rw_index,
        "expected read-only parent remount before nested writable bind: {:#?}",
        args.args
    );
}

#[test]
fn split_policy_reenables_writable_subpaths_after_unreadable_parent() {
    let temp_dir = TempDir::new().expect("temp dir");
    let blocked = temp_dir.path().join("blocked");
    let allowed = blocked.join("allowed");
    std::fs::create_dir_all(&allowed).expect("create blocked/allowed");
    let blocked = AbsolutePathBuf::from_absolute_path(&blocked).expect("absolute blocked");
    let allowed = AbsolutePathBuf::from_absolute_path(&allowed).expect("absolute allowed");
    let policy = FileSystemSandboxPolicy::restricted(vec![
        FileSystemSandboxEntry {
            path: FileSystemPath::Special {
                value: FileSystemSpecialPath::Root,
            },
            access: FileSystemAccessMode::Read,
        },
        FileSystemSandboxEntry {
            path: FileSystemPath::Path {
                path: blocked.clone(),
            },
            access: FileSystemAccessMode::None,
        },
        FileSystemSandboxEntry {
            path: FileSystemPath::Path {
                path: allowed.clone(),
            },
            access: FileSystemAccessMode::Write,
        },
    ]);

    let args = create_filesystem_args(&policy, temp_dir.path()).expect("filesystem args");
    let blocked_str = path_to_string(blocked.as_path());
    let allowed_str = path_to_string(allowed.as_path());
    let blocked_none_index = args
        .args
        .windows(4)
        .position(|window| window == ["--perms", "111", "--tmpfs", blocked_str.as_str()])
        .expect("blocked should be masked first");
    let allowed_dir_index = args
        .args
        .windows(2)
        .position(|window| window == ["--dir", allowed_str.as_str()])
        .expect("allowed mount target should be recreated");
    let blocked_remount_ro_index = args
        .args
        .windows(2)
        .position(|window| window == ["--remount-ro", blocked_str.as_str()])
        .expect("blocked directory should be remounted read-only");
    let allowed_bind_index = args
        .args
        .windows(3)
        .position(|window| window == ["--bind", allowed_str.as_str(), allowed_str.as_str()])
        .expect("allowed path should be rebound writable");

    assert!(
        blocked_none_index < allowed_dir_index
            && allowed_dir_index < blocked_remount_ro_index
            && blocked_remount_ro_index < allowed_bind_index,
        "expected writable child target recreation before remounting and rebinding under unreadable parent: {:#?}",
        args.args
    );
}

#[test]
fn split_policy_reenables_writable_files_after_unreadable_parent() {
    let temp_dir = TempDir::new().expect("temp dir");
    let blocked = temp_dir.path().join("blocked");
    let allowed_dir = blocked.join("allowed");
    let allowed_file = allowed_dir.join("note.txt");
    std::fs::create_dir_all(&allowed_dir).expect("create blocked/allowed");
    std::fs::write(&allowed_file, "ok").expect("create note");
    let blocked = AbsolutePathBuf::from_absolute_path(&blocked).expect("absolute blocked");
    let allowed_dir =
        AbsolutePathBuf::from_absolute_path(&allowed_dir).expect("absolute allowed dir");
    let allowed_file =
        AbsolutePathBuf::from_absolute_path(&allowed_file).expect("absolute allowed file");
    let policy = FileSystemSandboxPolicy::restricted(vec![
        FileSystemSandboxEntry {
            path: FileSystemPath::Special {
                value: FileSystemSpecialPath::Root,
            },
            access: FileSystemAccessMode::Read,
        },
        FileSystemSandboxEntry {
            path: FileSystemPath::Path {
                path: blocked.clone(),
            },
            access: FileSystemAccessMode::None,
        },
        FileSystemSandboxEntry {
            path: FileSystemPath::Path {
                path: allowed_file.clone(),
            },
            access: FileSystemAccessMode::Write,
        },
    ]);

    let args = create_filesystem_args(&policy, temp_dir.path()).expect("filesystem args");
    let blocked_str = path_to_string(blocked.as_path());
    let allowed_dir_str = path_to_string(allowed_dir.as_path());
    let allowed_file_str = path_to_string(allowed_file.as_path());

    assert!(
        args.args
            .windows(2)
            .any(|window| window == ["--dir", allowed_dir_str.as_str()]),
        "expected ancestor directory to be recreated: {:#?}",
        args.args
    );
    assert!(
        !args
            .args
            .windows(2)
            .any(|window| window == ["--dir", allowed_file_str.as_str()]),
        "writable file target should not be converted into a directory: {:#?}",
        args.args
    );
    let blocked_none_index = args
        .args
        .windows(4)
        .position(|window| window == ["--perms", "111", "--tmpfs", blocked_str.as_str()])
        .expect("blocked should be masked first");
    let allowed_bind_index = args
        .args
        .windows(3)
        .position(|window| {
            window
                == [
                    "--bind",
                    allowed_file_str.as_str(),
                    allowed_file_str.as_str(),
                ]
        })
        .expect("allowed file should be rebound writable");

    assert!(
        blocked_none_index < allowed_bind_index,
        "expected unreadable parent mask before rebinding writable file child: {:#?}",
        args.args
    );
}

#[test]
fn split_policy_reenables_nested_writable_roots_after_unreadable_parent() {
    let temp_dir = TempDir::new().expect("temp dir");
    let writable_root = temp_dir.path().join("workspace");
    let blocked = writable_root.join("blocked");
    let allowed = blocked.join("allowed");
    std::fs::create_dir_all(&allowed).expect("create blocked/allowed dir");
    let writable_root =
        AbsolutePathBuf::from_absolute_path(&writable_root).expect("absolute writable root");
    let blocked = AbsolutePathBuf::from_absolute_path(&blocked).expect("absolute blocked dir");
    let allowed = AbsolutePathBuf::from_absolute_path(&allowed).expect("absolute allowed dir");
    let blocked_str = path_to_string(blocked.as_path());
    let allowed_str = path_to_string(allowed.as_path());
    let policy = FileSystemSandboxPolicy::restricted(vec![
        FileSystemSandboxEntry {
            path: FileSystemPath::Path {
                path: writable_root,
            },
            access: FileSystemAccessMode::Write,
        },
        FileSystemSandboxEntry {
            path: FileSystemPath::Path { path: blocked },
            access: FileSystemAccessMode::None,
        },
        FileSystemSandboxEntry {
            path: FileSystemPath::Path { path: allowed },
            access: FileSystemAccessMode::Write,
        },
    ]);

    let args = create_filesystem_args(&policy, temp_dir.path()).expect("filesystem args");
    let blocked_none_index = args
        .args
        .windows(4)
        .position(|window| window == ["--perms", "111", "--tmpfs", blocked_str.as_str()])
        .expect("blocked should be masked first");
    let allowed_dir_index = args
        .args
        .windows(2)
        .position(|window| window == ["--dir", allowed_str.as_str()])
        .expect("allowed mount target should be recreated");
    let allowed_bind_index = args
        .args
        .windows(3)
        .position(|window| window == ["--bind", allowed_str.as_str(), allowed_str.as_str()])
        .expect("allowed path should be rebound writable");

    assert!(
        blocked_none_index < allowed_dir_index && allowed_dir_index < allowed_bind_index,
        "expected unreadable parent mask before recreating and rebinding writable child: {:#?}",
        args.args
    );
}

#[test]
fn split_policy_masks_root_read_directory_carveouts() {
    let temp_dir = TempDir::new().expect("temp dir");
    let blocked = temp_dir.path().join("blocked");
    std::fs::create_dir_all(&blocked).expect("create blocked dir");
    let blocked = AbsolutePathBuf::from_absolute_path(&blocked).expect("absolute blocked dir");
    let policy = FileSystemSandboxPolicy::restricted(vec![
        FileSystemSandboxEntry {
            path: FileSystemPath::Special {
                value: FileSystemSpecialPath::Root,
            },
            access: FileSystemAccessMode::Read,
        },
        FileSystemSandboxEntry {
            path: FileSystemPath::Path {
                path: blocked.clone(),
            },
            access: FileSystemAccessMode::None,
        },
    ]);

    let args = create_filesystem_args(&policy, temp_dir.path()).expect("filesystem args");
    let blocked_str = path_to_string(blocked.as_path());

    assert!(
        args.args
            .windows(3)
            .any(|window| window == ["--ro-bind", "/", "/"])
    );
    assert!(
        args.args
            .windows(4)
            .any(|window| { window == ["--perms", "000", "--tmpfs", blocked_str.as_str()] })
    );
    assert!(
        args.args
            .windows(2)
            .any(|window| window == ["--remount-ro", blocked_str.as_str()])
    );
}

#[test]
fn split_policy_masks_root_read_file_carveouts() {
    let temp_dir = TempDir::new().expect("temp dir");
    let blocked_file = temp_dir.path().join("blocked.txt");
    std::fs::write(&blocked_file, "secret").expect("create blocked file");
    let blocked_file =
        AbsolutePathBuf::from_absolute_path(&blocked_file).expect("absolute blocked file");
    let policy = FileSystemSandboxPolicy::restricted(vec![
        FileSystemSandboxEntry {
            path: FileSystemPath::Special {
                value: FileSystemSpecialPath::Root,
            },
            access: FileSystemAccessMode::Read,
        },
        FileSystemSandboxEntry {
            path: FileSystemPath::Path {
                path: blocked_file.clone(),
            },
            access: FileSystemAccessMode::None,
        },
    ]);

    let args = create_filesystem_args(&policy, temp_dir.path()).expect("filesystem args");
    let blocked_file_str = path_to_string(blocked_file.as_path());

    assert_eq!(args.preserved_files.len(), 1);
    assert!(args.args.windows(5).any(|window| {
        window[0] == "--perms"
            && window[1] == "000"
            && window[2] == "--ro-bind-data"
            && window[4] == blocked_file_str
    }));
}
