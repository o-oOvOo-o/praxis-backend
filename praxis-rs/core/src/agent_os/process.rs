use async_trait::async_trait;

mod tracking;

pub(crate) mod process_runtime_kind {
    pub(crate) const GENERIC: &str = "generic";
    pub(crate) const SHELL: &str = "shell";
    pub(crate) const ZSH_FORK: &str = "zsh_fork";
    pub(crate) const UNIFIED_EXEC: &str = "unified_exec";
    pub(crate) const LONG_PROCESS: &str = "long_process";
    pub(crate) const GPU_COMMAND: &str = "gpu_command";
    pub(crate) const NETWORK_COMMAND: &str = "network_command";
    pub(crate) const COMMAND: &str = "command";
    pub(crate) const APPLY_PATCH: &str = "apply_patch";
}

pub(crate) mod process_runtime_owner {
    pub(crate) const SHELL: &str = "shell-host";
    pub(crate) const ZSH_FORK: &str = "zsh-fork-host";
}

#[async_trait]
pub(crate) trait AgentOsProcessCleaner: Send + Sync {
    fn runtime_kind(&self) -> &'static str {
        process_runtime_kind::GENERIC
    }

    /// Stable backend identifier for the concrete runtime instance that owns
    /// process ids. Process ids are scoped to runtime backends; using only the
    /// numeric id is unsafe when multiple sessions/managers are live.
    fn runtime_owner_id(&self) -> String {
        self.runtime_kind().to_string()
    }

    async fn cleanup_agent_os_process(&self, process_id: i32) -> bool;
}

pub(super) fn process_registry_key(process_id: i32, runtime_owner_id: Option<&str>) -> String {
    match runtime_owner_id.filter(|owner| !owner.is_empty()) {
        Some(owner) => format!("{owner}:{process_id}"),
        None => format!("unscoped:{process_id}"),
    }
}

pub(super) fn cleaner_registry_key(runtime_kind: &str, runtime_owner_id: &str) -> String {
    format!("{runtime_kind}:{runtime_owner_id}")
}
