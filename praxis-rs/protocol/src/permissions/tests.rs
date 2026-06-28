use super::*;
use pretty_assertions::assert_eq;
#[cfg(unix)]
use std::fs;
use std::path::Path;
use tempfile::TempDir;

#[cfg(unix)]
const SYMLINKED_TMPDIR_TEST_ENV: &str = "PRAXIS_PROTOCOL_TEST_SYMLINKED_TMPDIR";

#[cfg(unix)]
fn symlink_dir(original: &Path, link: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(original, link)
}

#[cfg(unix)]
#[test]
fn writable_roots_proactively_protect_missing_dot_praxis() {
    let cwd = TempDir::new().expect("tempdir");
    let expected_root =
        AbsolutePathBuf::from_absolute_path(cwd.path().canonicalize().expect("canonicalize cwd"))
            .expect("absolute canonical root");
    let expected_dot_praxis = expected_root
        .join(".praxis")
        .expect("expected .praxis path");

    let policy = FileSystemSandboxPolicy::restricted(vec![FileSystemSandboxEntry {
        path: FileSystemPath::Special {
            value: FileSystemSpecialPath::CurrentWorkingDirectory,
        },
        access: FileSystemAccessMode::Write,
    }]);

    let writable_roots = policy.get_writable_roots_with_cwd(cwd.path());
    assert_eq!(writable_roots.len(), 1);
    assert_eq!(writable_roots[0].root, expected_root);
    assert!(
        writable_roots[0]
            .read_only_subpaths
            .contains(&expected_dot_praxis)
    );
}

#[cfg(unix)]
#[test]
fn writable_roots_skip_default_dot_praxis_when_explicit_user_rule_exists() {
    let cwd = TempDir::new().expect("tempdir");
    let expected_root =
        AbsolutePathBuf::from_absolute_path(cwd.path().canonicalize().expect("canonicalize cwd"))
            .expect("absolute canonical root");
    let explicit_dot_praxis = expected_root
        .join(".praxis")
        .expect("expected .praxis path");

    let policy = FileSystemSandboxPolicy::restricted(vec![
        FileSystemSandboxEntry {
            path: FileSystemPath::Special {
                value: FileSystemSpecialPath::CurrentWorkingDirectory,
            },
            access: FileSystemAccessMode::Write,
        },
        FileSystemSandboxEntry {
            path: FileSystemPath::Path {
                path: explicit_dot_praxis.clone(),
            },
            access: FileSystemAccessMode::Write,
        },
    ]);

    let writable_roots = policy.get_writable_roots_with_cwd(cwd.path());
    let workspace_root = writable_roots
        .iter()
        .find(|root| root.root == expected_root)
        .expect("workspace writable root");
    assert!(
        !workspace_root
            .read_only_subpaths
            .contains(&explicit_dot_praxis),
        "explicit .praxis rule should win over the default protected carveout"
    );
    assert!(
        policy.can_write_path_with_cwd(
            explicit_dot_praxis
                .join("config.toml")
                .expect("config.toml")
                .as_path(),
            cwd.path()
        )
    );
}

#[test]
fn workspace_write_projection_projection_blocks_missing_dot_praxis_writes() {
    let cwd = TempDir::new().expect("tempdir");
    let dot_praxis_config = cwd.path().join(".praxis").join("config.toml");
    let policy = SandboxPolicy::WorkspaceWrite {
        writable_roots: vec![],
        read_only_access: ReadOnlyAccess::Restricted {
            include_platform_defaults: false,
            readable_roots: vec![],
        },
        network_access: false,
        exclude_tmpdir_env_var: true,
        exclude_slash_tmp: true,
    };

    let file_system_policy = FileSystemSandboxPolicy::from_sandbox_policy(&policy, cwd.path());

    assert!(!file_system_policy.can_write_path_with_cwd(&dot_praxis_config, cwd.path()));
}

#[test]
fn workspace_write_projection_projection_accepts_relative_cwd() {
    let relative_cwd = Path::new("workspace");
    let expected_dot_praxis = AbsolutePathBuf::from_absolute_path(
        std::env::current_dir()
            .expect("current dir")
            .join(relative_cwd)
            .join(".praxis"),
    )
    .expect("absolute dot praxis");
    let policy = SandboxPolicy::WorkspaceWrite {
        writable_roots: vec![],
        read_only_access: ReadOnlyAccess::Restricted {
            include_platform_defaults: false,
            readable_roots: vec![],
        },
        network_access: false,
        exclude_tmpdir_env_var: true,
        exclude_slash_tmp: true,
    };

    let file_system_policy = FileSystemSandboxPolicy::from_sandbox_policy(&policy, relative_cwd);

    assert_eq!(
        file_system_policy,
        FileSystemSandboxPolicy::restricted(vec![
            FileSystemSandboxEntry {
                path: FileSystemPath::Special {
                    value: FileSystemSpecialPath::CurrentWorkingDirectory,
                },
                access: FileSystemAccessMode::Write,
            },
            FileSystemSandboxEntry {
                path: FileSystemPath::Path {
                    path: expected_dot_praxis,
                },
                access: FileSystemAccessMode::Read,
            },
        ])
    );
    assert!(
        !file_system_policy
            .can_write_path_with_cwd(Path::new(".praxis/config.toml"), relative_cwd,)
    );
}

#[cfg(unix)]
#[test]
fn effective_runtime_roots_canonicalize_symlinked_paths() {
    let cwd = TempDir::new().expect("tempdir");
    let real_root = cwd.path().join("real");
    let link_root = cwd.path().join("link");
    let blocked = real_root.join("blocked");
    let praxis_dir = real_root.join(".praxis");

    fs::create_dir_all(&blocked).expect("create blocked");
    fs::create_dir_all(&praxis_dir).expect("create .praxis");
    symlink_dir(&real_root, &link_root).expect("create symlinked root");

    let link_root =
        AbsolutePathBuf::from_absolute_path(&link_root).expect("absolute symlinked root");
    let link_blocked = link_root.join("blocked").expect("symlinked blocked path");
    let expected_root = AbsolutePathBuf::from_absolute_path(
        real_root.canonicalize().expect("canonicalize real root"),
    )
    .expect("absolute canonical root");
    let expected_blocked =
        AbsolutePathBuf::from_absolute_path(blocked.canonicalize().expect("canonicalize blocked"))
            .expect("absolute canonical blocked");
    let expected_praxis = AbsolutePathBuf::from_absolute_path(
        praxis_dir.canonicalize().expect("canonicalize .praxis"),
    )
    .expect("absolute canonical .praxis");

    let policy = FileSystemSandboxPolicy::restricted(vec![
        FileSystemSandboxEntry {
            path: FileSystemPath::Path { path: link_root },
            access: FileSystemAccessMode::Write,
        },
        FileSystemSandboxEntry {
            path: FileSystemPath::Path { path: link_blocked },
            access: FileSystemAccessMode::None,
        },
    ]);

    assert_eq!(
        policy.get_unreadable_roots_with_cwd(cwd.path()),
        vec![expected_blocked.clone()]
    );

    let writable_roots = policy.get_writable_roots_with_cwd(cwd.path());
    assert_eq!(writable_roots.len(), 1);
    assert_eq!(writable_roots[0].root, expected_root);
    assert!(
        writable_roots[0]
            .read_only_subpaths
            .contains(&expected_blocked)
    );
    assert!(
        writable_roots[0]
            .read_only_subpaths
            .contains(&expected_praxis)
    );
}

#[cfg(unix)]
#[test]
fn current_working_directory_special_path_canonicalizes_symlinked_cwd() {
    let cwd = TempDir::new().expect("tempdir");
    let real_root = cwd.path().join("real");
    let link_root = cwd.path().join("link");
    let blocked = real_root.join("blocked");
    let agents_dir = real_root.join(".agents");
    let praxis_dir = real_root.join(".praxis");

    fs::create_dir_all(&blocked).expect("create blocked");
    fs::create_dir_all(&agents_dir).expect("create .agents");
    fs::create_dir_all(&praxis_dir).expect("create .praxis");
    symlink_dir(&real_root, &link_root).expect("create symlinked cwd");

    let link_blocked =
        AbsolutePathBuf::from_absolute_path(link_root.join("blocked")).expect("link blocked");
    let expected_root = AbsolutePathBuf::from_absolute_path(
        real_root.canonicalize().expect("canonicalize real root"),
    )
    .expect("absolute canonical root");
    let expected_blocked =
        AbsolutePathBuf::from_absolute_path(blocked.canonicalize().expect("canonicalize blocked"))
            .expect("absolute canonical blocked");
    let expected_agents = AbsolutePathBuf::from_absolute_path(
        agents_dir.canonicalize().expect("canonicalize .agents"),
    )
    .expect("absolute canonical .agents");
    let expected_praxis = AbsolutePathBuf::from_absolute_path(
        praxis_dir.canonicalize().expect("canonicalize .praxis"),
    )
    .expect("absolute canonical .praxis");

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
        FileSystemSandboxEntry {
            path: FileSystemPath::Path { path: link_blocked },
            access: FileSystemAccessMode::None,
        },
    ]);

    assert_eq!(
        policy.get_readable_roots_with_cwd(&link_root),
        vec![expected_root.clone()]
    );
    assert_eq!(
        policy.get_unreadable_roots_with_cwd(&link_root),
        vec![expected_blocked.clone()]
    );

    let writable_roots = policy.get_writable_roots_with_cwd(&link_root);
    assert_eq!(writable_roots.len(), 1);
    assert_eq!(writable_roots[0].root, expected_root);
    assert!(
        writable_roots[0]
            .read_only_subpaths
            .contains(&expected_blocked)
    );
    assert!(
        writable_roots[0]
            .read_only_subpaths
            .contains(&expected_agents)
    );
    assert!(
        writable_roots[0]
            .read_only_subpaths
            .contains(&expected_praxis)
    );
}

#[cfg(unix)]
#[test]
fn writable_roots_preserve_symlinked_protected_subpaths() {
    let cwd = TempDir::new().expect("tempdir");
    let root = cwd.path().join("root");
    let decoy = root.join("decoy-praxis");
    let dot_praxis = root.join(".praxis");
    fs::create_dir_all(&decoy).expect("create decoy");
    symlink_dir(&decoy, &dot_praxis).expect("create .praxis symlink");

    let root = AbsolutePathBuf::from_absolute_path(&root).expect("absolute root");
    let expected_dot_praxis = AbsolutePathBuf::from_absolute_path(
        root.as_path()
            .canonicalize()
            .expect("canonicalize root")
            .join(".praxis"),
    )
    .expect("absolute .praxis symlink");
    let unexpected_decoy =
        AbsolutePathBuf::from_absolute_path(decoy.canonicalize().expect("canonicalize decoy"))
            .expect("absolute canonical decoy");

    let policy = FileSystemSandboxPolicy::restricted(vec![FileSystemSandboxEntry {
        path: FileSystemPath::Path { path: root },
        access: FileSystemAccessMode::Write,
    }]);

    let writable_roots = policy.get_writable_roots_with_cwd(cwd.path());
    assert_eq!(writable_roots.len(), 1);
    assert_eq!(
        writable_roots[0].read_only_subpaths,
        vec![expected_dot_praxis]
    );
    assert!(
        !writable_roots[0]
            .read_only_subpaths
            .contains(&unexpected_decoy)
    );
}

#[cfg(unix)]
#[test]
fn writable_roots_preserve_explicit_symlinked_carveouts_under_symlinked_roots() {
    let cwd = TempDir::new().expect("tempdir");
    let real_root = cwd.path().join("real");
    let link_root = cwd.path().join("link");
    let decoy = real_root.join("decoy-private");
    let linked_private = real_root.join("linked-private");
    fs::create_dir_all(&decoy).expect("create decoy");
    symlink_dir(&real_root, &link_root).expect("create symlinked root");
    symlink_dir(&decoy, &linked_private).expect("create linked-private symlink");

    let link_root =
        AbsolutePathBuf::from_absolute_path(&link_root).expect("absolute symlinked root");
    let link_private = link_root
        .join("linked-private")
        .expect("symlinked linked-private path");
    let expected_root = AbsolutePathBuf::from_absolute_path(
        real_root.canonicalize().expect("canonicalize real root"),
    )
    .expect("absolute canonical root");
    let expected_linked_private = expected_root
        .join("linked-private")
        .expect("expected linked-private path");
    let unexpected_decoy =
        AbsolutePathBuf::from_absolute_path(decoy.canonicalize().expect("canonicalize decoy"))
            .expect("absolute canonical decoy");

    let policy = FileSystemSandboxPolicy::restricted(vec![
        FileSystemSandboxEntry {
            path: FileSystemPath::Path { path: link_root },
            access: FileSystemAccessMode::Write,
        },
        FileSystemSandboxEntry {
            path: FileSystemPath::Path { path: link_private },
            access: FileSystemAccessMode::None,
        },
    ]);

    let writable_roots = policy.get_writable_roots_with_cwd(cwd.path());
    assert_eq!(writable_roots.len(), 1);
    assert_eq!(writable_roots[0].root, expected_root);
    assert_eq!(
        writable_roots[0].read_only_subpaths,
        vec![expected_linked_private]
    );
    assert!(
        !writable_roots[0]
            .read_only_subpaths
            .contains(&unexpected_decoy)
    );
}

#[cfg(unix)]
#[test]
fn writable_roots_preserve_explicit_symlinked_carveouts_that_escape_root() {
    let cwd = TempDir::new().expect("tempdir");
    let real_root = cwd.path().join("real");
    let link_root = cwd.path().join("link");
    let decoy = cwd.path().join("outside-private");
    let linked_private = real_root.join("linked-private");
    fs::create_dir_all(&decoy).expect("create decoy");
    fs::create_dir_all(&real_root).expect("create real root");
    symlink_dir(&real_root, &link_root).expect("create symlinked root");
    symlink_dir(&decoy, &linked_private).expect("create linked-private symlink");

    let link_root =
        AbsolutePathBuf::from_absolute_path(&link_root).expect("absolute symlinked root");
    let link_private = link_root
        .join("linked-private")
        .expect("symlinked linked-private path");
    let expected_root = AbsolutePathBuf::from_absolute_path(
        real_root.canonicalize().expect("canonicalize real root"),
    )
    .expect("absolute canonical root");
    let expected_linked_private = expected_root
        .join("linked-private")
        .expect("expected linked-private path");
    let unexpected_decoy =
        AbsolutePathBuf::from_absolute_path(decoy.canonicalize().expect("canonicalize decoy"))
            .expect("absolute canonical decoy");

    let policy = FileSystemSandboxPolicy::restricted(vec![
        FileSystemSandboxEntry {
            path: FileSystemPath::Path { path: link_root },
            access: FileSystemAccessMode::Write,
        },
        FileSystemSandboxEntry {
            path: FileSystemPath::Path { path: link_private },
            access: FileSystemAccessMode::None,
        },
    ]);

    let writable_roots = policy.get_writable_roots_with_cwd(cwd.path());
    assert_eq!(writable_roots.len(), 1);
    assert_eq!(writable_roots[0].root, expected_root);
    assert_eq!(
        writable_roots[0].read_only_subpaths,
        vec![expected_linked_private]
    );
    assert!(
        !writable_roots[0]
            .read_only_subpaths
            .contains(&unexpected_decoy)
    );
}

#[cfg(unix)]
#[test]
fn writable_roots_preserve_explicit_symlinked_carveouts_that_alias_root() {
    let cwd = TempDir::new().expect("tempdir");
    let root = cwd.path().join("root");
    let alias = root.join("alias-root");
    fs::create_dir_all(&root).expect("create root");
    symlink_dir(&root, &alias).expect("create alias symlink");

    let root = AbsolutePathBuf::from_absolute_path(&root).expect("absolute root");
    let alias = root.join("alias-root").expect("alias root path");
    let expected_root = AbsolutePathBuf::from_absolute_path(
        root.as_path().canonicalize().expect("canonicalize root"),
    )
    .expect("absolute canonical root");
    let expected_alias = expected_root
        .join("alias-root")
        .expect("expected alias path");

    let policy = FileSystemSandboxPolicy::restricted(vec![
        FileSystemSandboxEntry {
            path: FileSystemPath::Path { path: root },
            access: FileSystemAccessMode::Write,
        },
        FileSystemSandboxEntry {
            path: FileSystemPath::Path { path: alias },
            access: FileSystemAccessMode::None,
        },
    ]);

    let writable_roots = policy.get_writable_roots_with_cwd(cwd.path());
    assert_eq!(writable_roots.len(), 1);
    assert_eq!(writable_roots[0].root, expected_root);
    assert_eq!(writable_roots[0].read_only_subpaths, vec![expected_alias]);
}

#[cfg(unix)]
#[test]
fn tmpdir_special_path_canonicalizes_symlinked_tmpdir() {
    if std::env::var_os(SYMLINKED_TMPDIR_TEST_ENV).is_none() {
        let output = std::process::Command::new(std::env::current_exe().expect("test binary"))
            .env(SYMLINKED_TMPDIR_TEST_ENV, "1")
            .arg("--exact")
            .arg("permissions::tests::tmpdir_special_path_canonicalizes_symlinked_tmpdir")
            .output()
            .expect("run tmpdir subprocess test");

        assert!(
            output.status.success(),
            "tmpdir subprocess test failed\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }

    let cwd = TempDir::new().expect("tempdir");
    let real_tmpdir = cwd.path().join("real-tmpdir");
    let link_tmpdir = cwd.path().join("link-tmpdir");
    let blocked = real_tmpdir.join("blocked");
    let praxis_dir = real_tmpdir.join(".praxis");

    fs::create_dir_all(&blocked).expect("create blocked");
    fs::create_dir_all(&praxis_dir).expect("create .praxis");
    symlink_dir(&real_tmpdir, &link_tmpdir).expect("create symlinked tmpdir");

    let link_blocked =
        AbsolutePathBuf::from_absolute_path(link_tmpdir.join("blocked")).expect("link blocked");
    let expected_root = AbsolutePathBuf::from_absolute_path(
        real_tmpdir
            .canonicalize()
            .expect("canonicalize real tmpdir"),
    )
    .expect("absolute canonical tmpdir");
    let expected_blocked =
        AbsolutePathBuf::from_absolute_path(blocked.canonicalize().expect("canonicalize blocked"))
            .expect("absolute canonical blocked");
    let expected_praxis = AbsolutePathBuf::from_absolute_path(
        praxis_dir.canonicalize().expect("canonicalize .praxis"),
    )
    .expect("absolute canonical .praxis");

    unsafe {
        std::env::set_var("TMPDIR", &link_tmpdir);
    }

    let policy = FileSystemSandboxPolicy::restricted(vec![
        FileSystemSandboxEntry {
            path: FileSystemPath::Special {
                value: FileSystemSpecialPath::Tmpdir,
            },
            access: FileSystemAccessMode::Write,
        },
        FileSystemSandboxEntry {
            path: FileSystemPath::Path { path: link_blocked },
            access: FileSystemAccessMode::None,
        },
    ]);

    assert_eq!(
        policy.get_unreadable_roots_with_cwd(cwd.path()),
        vec![expected_blocked.clone()]
    );

    let writable_roots = policy.get_writable_roots_with_cwd(cwd.path());
    assert_eq!(writable_roots.len(), 1);
    assert_eq!(writable_roots[0].root, expected_root);
    assert!(
        writable_roots[0]
            .read_only_subpaths
            .contains(&expected_blocked)
    );
    assert!(
        writable_roots[0]
            .read_only_subpaths
            .contains(&expected_praxis)
    );
}

#[test]
fn resolve_access_with_cwd_uses_most_specific_entry() {
    let cwd = TempDir::new().expect("tempdir");
    let docs =
        AbsolutePathBuf::resolve_path_against_base("docs", cwd.path()).expect("resolve docs");
    let docs_private = AbsolutePathBuf::resolve_path_against_base("docs/private", cwd.path())
        .expect("resolve docs/private");
    let docs_private_public =
        AbsolutePathBuf::resolve_path_against_base("docs/private/public", cwd.path())
            .expect("resolve docs/private/public");
    let policy = FileSystemSandboxPolicy::restricted(vec![
        FileSystemSandboxEntry {
            path: FileSystemPath::Special {
                value: FileSystemSpecialPath::CurrentWorkingDirectory,
            },
            access: FileSystemAccessMode::Write,
        },
        FileSystemSandboxEntry {
            path: FileSystemPath::Path { path: docs.clone() },
            access: FileSystemAccessMode::Read,
        },
        FileSystemSandboxEntry {
            path: FileSystemPath::Path {
                path: docs_private.clone(),
            },
            access: FileSystemAccessMode::None,
        },
        FileSystemSandboxEntry {
            path: FileSystemPath::Path {
                path: docs_private_public.clone(),
            },
            access: FileSystemAccessMode::Write,
        },
    ]);

    assert_eq!(
        policy.resolve_access_with_cwd(cwd.path(), cwd.path()),
        FileSystemAccessMode::Write
    );
    assert_eq!(
        policy.resolve_access_with_cwd(docs.as_path(), cwd.path()),
        FileSystemAccessMode::Read
    );
    assert_eq!(
        policy.resolve_access_with_cwd(docs_private.as_path(), cwd.path()),
        FileSystemAccessMode::None
    );
    assert_eq!(
        policy.resolve_access_with_cwd(docs_private_public.as_path(), cwd.path()),
        FileSystemAccessMode::Write
    );
}

#[test]
fn split_only_nested_carveouts_need_direct_runtime_enforcement() {
    let cwd = TempDir::new().expect("tempdir");
    let docs =
        AbsolutePathBuf::resolve_path_against_base("docs", cwd.path()).expect("resolve docs");
    let policy = FileSystemSandboxPolicy::restricted(vec![
        FileSystemSandboxEntry {
            path: FileSystemPath::Special {
                value: FileSystemSpecialPath::CurrentWorkingDirectory,
            },
            access: FileSystemAccessMode::Write,
        },
        FileSystemSandboxEntry {
            path: FileSystemPath::Path { path: docs },
            access: FileSystemAccessMode::Read,
        },
    ]);

    assert!(policy.needs_direct_runtime_enforcement(NetworkSandboxPolicy::Restricted, cwd.path(),));

    let workspace_write_projection = FileSystemSandboxPolicy::from_sandbox_policy(
        &SandboxPolicy::new_workspace_write_policy(),
        cwd.path(),
    );
    assert!(
        !workspace_write_projection
            .needs_direct_runtime_enforcement(NetworkSandboxPolicy::Restricted, cwd.path(),)
    );
}

#[test]
fn root_write_with_read_only_child_is_not_full_disk_write() {
    let cwd = TempDir::new().expect("tempdir");
    let docs =
        AbsolutePathBuf::resolve_path_against_base("docs", cwd.path()).expect("resolve docs");
    let policy = FileSystemSandboxPolicy::restricted(vec![
        FileSystemSandboxEntry {
            path: FileSystemPath::Special {
                value: FileSystemSpecialPath::Root,
            },
            access: FileSystemAccessMode::Write,
        },
        FileSystemSandboxEntry {
            path: FileSystemPath::Path { path: docs.clone() },
            access: FileSystemAccessMode::Read,
        },
    ]);

    assert!(!policy.has_full_disk_write_access());
    assert_eq!(
        policy.resolve_access_with_cwd(docs.as_path(), cwd.path()),
        FileSystemAccessMode::Read
    );
    assert!(policy.needs_direct_runtime_enforcement(NetworkSandboxPolicy::Restricted, cwd.path(),));
}

#[test]
fn root_deny_does_not_materialize_as_unreadable_root() {
    let cwd = TempDir::new().expect("tempdir");
    let docs =
        AbsolutePathBuf::resolve_path_against_base("docs", cwd.path()).expect("resolve docs");
    let expected_docs = AbsolutePathBuf::from_absolute_path(
        cwd.path()
            .canonicalize()
            .expect("canonicalize cwd")
            .join("docs"),
    )
    .expect("canonical docs");
    let policy = FileSystemSandboxPolicy::restricted(vec![
        FileSystemSandboxEntry {
            path: FileSystemPath::Special {
                value: FileSystemSpecialPath::Root,
            },
            access: FileSystemAccessMode::None,
        },
        FileSystemSandboxEntry {
            path: FileSystemPath::Path { path: docs.clone() },
            access: FileSystemAccessMode::Read,
        },
    ]);

    assert_eq!(
        policy.resolve_access_with_cwd(docs.as_path(), cwd.path()),
        FileSystemAccessMode::Read
    );
    assert_eq!(
        policy.get_readable_roots_with_cwd(cwd.path()),
        vec![expected_docs]
    );
    assert!(policy.get_unreadable_roots_with_cwd(cwd.path()).is_empty());
}

#[test]
fn duplicate_root_deny_prevents_full_disk_write_access() {
    let cwd = TempDir::new().expect("tempdir");
    let root = AbsolutePathBuf::from_absolute_path(cwd.path())
        .map(|cwd| absolute_root_path_for_cwd(&cwd))
        .expect("resolve filesystem root");
    let policy = FileSystemSandboxPolicy::restricted(vec![
        FileSystemSandboxEntry {
            path: FileSystemPath::Special {
                value: FileSystemSpecialPath::Root,
            },
            access: FileSystemAccessMode::Write,
        },
        FileSystemSandboxEntry {
            path: FileSystemPath::Special {
                value: FileSystemSpecialPath::Root,
            },
            access: FileSystemAccessMode::None,
        },
    ]);

    assert!(!policy.has_full_disk_write_access());
    assert_eq!(
        policy.resolve_access_with_cwd(root.as_path(), cwd.path()),
        FileSystemAccessMode::None
    );
}

#[test]
fn same_specificity_write_override_keeps_full_disk_write_access() {
    let cwd = TempDir::new().expect("tempdir");
    let docs =
        AbsolutePathBuf::resolve_path_against_base("docs", cwd.path()).expect("resolve docs");
    let policy = FileSystemSandboxPolicy::restricted(vec![
        FileSystemSandboxEntry {
            path: FileSystemPath::Special {
                value: FileSystemSpecialPath::Root,
            },
            access: FileSystemAccessMode::Write,
        },
        FileSystemSandboxEntry {
            path: FileSystemPath::Path { path: docs.clone() },
            access: FileSystemAccessMode::Read,
        },
        FileSystemSandboxEntry {
            path: FileSystemPath::Path { path: docs.clone() },
            access: FileSystemAccessMode::Write,
        },
    ]);

    assert!(policy.has_full_disk_write_access());
    assert_eq!(
        policy.resolve_access_with_cwd(docs.as_path(), cwd.path()),
        FileSystemAccessMode::Write
    );
}

#[test]
fn with_additional_readable_roots_skips_existing_effective_access() {
    let cwd = TempDir::new().expect("tempdir");
    let cwd_root = AbsolutePathBuf::from_absolute_path(cwd.path()).expect("absolute cwd");
    let policy = FileSystemSandboxPolicy::restricted(vec![FileSystemSandboxEntry {
        path: FileSystemPath::Special {
            value: FileSystemSpecialPath::CurrentWorkingDirectory,
        },
        access: FileSystemAccessMode::Read,
    }]);

    let actual = policy
        .clone()
        .with_additional_readable_roots(cwd.path(), std::slice::from_ref(&cwd_root));

    assert_eq!(actual, policy);
}

#[test]
fn with_additional_writable_roots_skips_existing_effective_access() {
    let cwd = TempDir::new().expect("tempdir");
    let cwd_root = AbsolutePathBuf::from_absolute_path(cwd.path()).expect("absolute cwd");
    let policy = FileSystemSandboxPolicy::restricted(vec![FileSystemSandboxEntry {
        path: FileSystemPath::Special {
            value: FileSystemSpecialPath::CurrentWorkingDirectory,
        },
        access: FileSystemAccessMode::Write,
    }]);

    let actual = policy
        .clone()
        .with_additional_writable_roots(cwd.path(), std::slice::from_ref(&cwd_root));

    assert_eq!(actual, policy);
}

#[test]
fn with_additional_writable_roots_adds_new_root() {
    let temp_dir = TempDir::new().expect("tempdir");
    let cwd = temp_dir.path().join("workspace");
    let extra = AbsolutePathBuf::from_absolute_path(temp_dir.path().join("extra"))
        .expect("resolve extra root");
    let policy = FileSystemSandboxPolicy::restricted(vec![FileSystemSandboxEntry {
        path: FileSystemPath::Special {
            value: FileSystemSpecialPath::CurrentWorkingDirectory,
        },
        access: FileSystemAccessMode::Write,
    }]);

    let actual = policy.with_additional_writable_roots(&cwd, std::slice::from_ref(&extra));

    assert_eq!(
        actual,
        FileSystemSandboxPolicy::restricted(vec![
            FileSystemSandboxEntry {
                path: FileSystemPath::Special {
                    value: FileSystemSpecialPath::CurrentWorkingDirectory,
                },
                access: FileSystemAccessMode::Write,
            },
            FileSystemSandboxEntry {
                path: FileSystemPath::Path { path: extra },
                access: FileSystemAccessMode::Write,
            },
        ])
    );
}

#[test]
fn file_system_access_mode_orders_by_conflict_precedence() {
    assert!(FileSystemAccessMode::Write > FileSystemAccessMode::Read);
    assert!(FileSystemAccessMode::None > FileSystemAccessMode::Write);
}
