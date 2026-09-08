use super::*;

#[test]
fn required_isolation_prevents_building_an_unsandboxed_exec_request() {
    let cwd = std::env::current_dir().expect("current directory");
    let result = build_exec_request(
        ExecParams {
            command: vec!["unused-program".to_string()],
            cwd: cwd.clone(),
            expiration: ExecExpiration::DefaultTimeout,
            capture_policy: ExecCapturePolicy::ShellTool,
            env: HashMap::new(),
            network: None,
            sandbox_permissions: SandboxPermissions::UseDefault,
            windows_sandbox_level: WindowsSandboxLevel::Disabled,
            windows_sandbox_private_desktop: false,
            justification: None,
            arg0: None,
        },
        &SandboxPolicy::ReadOnly {
            access: praxis_protocol::protocol::ReadOnlyAccess::FullAccess,
            network_access: false,
        },
        &FileSystemSandboxPolicy::unrestricted(),
        NetworkSandboxPolicy::Restricted,
        &cwd,
        /*praxis_linux_sandbox_exe*/ &None,
        /*use_legacy_landlock*/ false,
    );
    assert!(matches!(result, Err(PraxisErr::UnsupportedOperation(_))));
}
