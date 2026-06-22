use super::intent::ActionIntentKind;
use serde::Deserialize;
use serde::Serialize;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct CapabilityProfile {
    pub(crate) profile_id: String,
    pub(crate) can_read_files: bool,
    pub(crate) can_write_files: bool,
    pub(crate) can_run_shell: bool,
    pub(crate) can_cpu_heavy: bool,
    pub(crate) can_compile: bool,
    pub(crate) can_run_app: bool,
    pub(crate) can_use_gpu: bool,
    pub(crate) can_hold_ports: bool,
    pub(crate) can_network: bool,
    pub(crate) can_modify_git: bool,
    pub(crate) can_spawn_long_process: bool,
    pub(crate) path_scopes: ScopedPaths,
    pub(crate) intent_scopes: ScopedIntents,
    pub(crate) command_denylist: Vec<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub(crate) struct ScopedPaths {
    pub(crate) allow: Vec<String>,
    pub(crate) deny: Vec<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub(crate) struct ScopedIntents {
    pub(crate) allow: Vec<ActionIntentKind>,
    pub(crate) deny: Vec<ActionIntentKind>,
}
