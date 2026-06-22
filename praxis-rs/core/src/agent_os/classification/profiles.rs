use crate::agent_os::ActionIntentKind;
use crate::agent_os::CapabilityProfile;
use crate::agent_os::ScopedIntents;
use crate::agent_os::ScopedPaths;

pub(in crate::agent_os) fn builtin_profiles() -> Vec<CapabilityProfile> {
    vec![
        CapabilityProfile {
            profile_id: "coordinator".to_string(),
            can_read_files: true,
            can_write_files: true,
            can_run_shell: true,
            can_cpu_heavy: true,
            can_compile: true,
            can_run_app: true,
            can_use_gpu: true,
            can_hold_ports: true,
            can_network: true,
            can_modify_git: true,
            can_spawn_long_process: true,
            path_scopes: ScopedPaths {
                allow: vec!["**".to_string()],
                deny: Vec::new(),
            },
            intent_scopes: ScopedIntents::default(),
            command_denylist: dangerous_command_denylist(),
        },
        CapabilityProfile {
            profile_id: "worker".to_string(),
            can_read_files: true,
            can_write_files: true,
            can_run_shell: true,
            can_cpu_heavy: false,
            can_compile: false,
            can_run_app: false,
            can_use_gpu: true,
            can_hold_ports: false,
            can_network: false,
            can_modify_git: false,
            can_spawn_long_process: false,
            path_scopes: ScopedPaths {
                allow: vec!["**".to_string()],
                deny: vec!["state/migrations/**".to_string(), ".github/**".to_string()],
            },
            intent_scopes: ScopedIntents {
                allow: Vec::new(),
                deny: vec![
                    ActionIntentKind::Compile,
                    ActionIntentKind::Test,
                    ActionIntentKind::RunApp,
                    ActionIntentKind::LongProcess,
                    ActionIntentKind::GitMutation,
                    ActionIntentKind::Network,
                ],
            },
            command_denylist: dangerous_command_denylist(),
        },
    ]
}

fn dangerous_command_denylist() -> Vec<String> {
    vec![
        "rm -rf /".to_string(),
        "curl | sh".to_string(),
        "wget | sh".to_string(),
        "sudo ".to_string(),
        "chmod -r 777".to_string(),
        "git reset --hard".to_string(),
    ]
}
