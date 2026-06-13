use super::classification::{
    builtin_profiles, denylist_surface, requires_compile, requires_cpu_heavy, requires_write,
};
use super::{
    ActionIntent, ActionIntentKind, AgentOsState, CapabilityProfile, ResourceRequirement,
    ScopedPaths,
};
use crate::path_scope::{normalize_path_for_scope, scope_matches};
use std::path::Path;

impl AgentOsState {
    pub(super) fn ensure_builtin_profiles(&mut self) {
        if self.profiles.is_empty() {
            for profile in builtin_profiles() {
                self.profiles.insert(profile.profile_id.clone(), profile);
            }
        }
    }
}

impl CapabilityProfile {
    pub(super) fn validate_command_intent(
        &self,
        intent: &ActionIntent,
        command: &[String],
        cwd: &Path,
    ) -> Result<(), String> {
        if self.command_denies(command) {
            return Err("command denied by AgentOS command denylist".to_string());
        }
        if !self.can_run_shell {
            return Err("profile cannot run shell commands".to_string());
        }
        if self.intent_scopes.deny.contains(&intent.kind) {
            return Err(format!("intent `{}` is denied", intent.kind.as_str()));
        }
        if !self.intent_scopes.allow.is_empty() && !self.intent_scopes.allow.contains(&intent.kind)
        {
            return Err(format!(
                "intent `{}` is outside allowed intents",
                intent.kind.as_str()
            ));
        }
        if requires_write(intent.kind) && !self.can_write_files {
            return Err("profile cannot write files".to_string());
        }
        if requires_compile(intent.kind) && !self.can_compile {
            return Err("profile cannot compile".to_string());
        }
        if requires_cpu_heavy(intent.kind) && !self.can_cpu_heavy {
            return Err("profile cannot use CPU-heavy execution".to_string());
        }
        if intent.kind == ActionIntentKind::RunApp && !self.can_run_app {
            return Err("profile cannot run app runtimes".to_string());
        }
        if intent.kind == ActionIntentKind::Gpu && !self.can_use_gpu {
            return Err("profile cannot use GPU resources".to_string());
        }
        if intent
            .required_resources
            .iter()
            .any(|resource| matches!(resource, ResourceRequirement::Gpu { .. }))
            && !self.can_use_gpu
        {
            return Err("profile cannot use GPU resources".to_string());
        }
        if intent
            .required_resources
            .iter()
            .any(|resource| matches!(resource, ResourceRequirement::Port { .. }))
            && !self.can_hold_ports
        {
            return Err("profile cannot hold ports".to_string());
        }
        if intent.kind == ActionIntentKind::Network && !self.can_network {
            return Err("profile cannot use network resources".to_string());
        }
        if intent.kind == ActionIntentKind::GitMutation && !self.can_modify_git {
            return Err("profile cannot modify git state".to_string());
        }
        if intent.kind == ActionIntentKind::LongProcess && !self.can_spawn_long_process {
            return Err("profile cannot spawn long-running processes".to_string());
        }
        if !self.path_scopes.allows(cwd) {
            return Err(format!(
                "cwd `{}` is outside profile path scope",
                cwd.display()
            ));
        }
        Ok(())
    }

    pub(super) fn validate_tool_intent(&self, intent: &ActionIntent) -> Result<(), String> {
        if self.intent_scopes.deny.contains(&intent.kind) {
            return Err(format!("intent `{}` is denied", intent.kind.as_str()));
        }
        if !self.intent_scopes.allow.is_empty() && !self.intent_scopes.allow.contains(&intent.kind)
        {
            return Err(format!(
                "intent `{}` is outside allowed intents",
                intent.kind.as_str()
            ));
        }
        if intent
            .required_resources
            .iter()
            .any(|resource| matches!(resource, ResourceRequirement::RepoWrite { .. }))
            && !self.can_write_files
        {
            return Err("profile cannot write files".to_string());
        }
        if intent
            .required_resources
            .iter()
            .any(|resource| matches!(resource, ResourceRequirement::Network { .. }))
            && !self.can_network
        {
            return Err("profile cannot use network or external side-effect tools".to_string());
        }
        if intent
            .required_resources
            .iter()
            .any(|resource| matches!(resource, ResourceRequirement::Gpu { .. }))
            && !self.can_use_gpu
        {
            return Err("profile cannot use GPU resources".to_string());
        }
        if intent
            .required_resources
            .iter()
            .any(|resource| matches!(resource, ResourceRequirement::Port { .. }))
            && !self.can_hold_ports
        {
            return Err("profile cannot hold ports".to_string());
        }
        if intent
            .required_resources
            .iter()
            .any(|resource| matches!(resource, ResourceRequirement::GitIndex { .. }))
            && !self.can_modify_git
        {
            return Err("profile cannot modify git".to_string());
        }
        Ok(())
    }

    fn command_denies(&self, command: &[String]) -> bool {
        let rendered = denylist_surface(command).to_ascii_lowercase();
        self.command_denylist
            .iter()
            .any(|pattern| rendered.contains(&pattern.to_ascii_lowercase()))
    }

    pub(super) fn capability_names_for_action(&self, action: &ActionIntent) -> Vec<String> {
        let intent = action.kind;
        let mut caps = vec!["run_shell".to_string()];
        if requires_write(intent) {
            caps.push("write_files".to_string());
        }
        if requires_compile(intent) {
            caps.push("compile".to_string());
        }
        if requires_cpu_heavy(intent) {
            caps.push("cpu_heavy".to_string());
        }
        if intent == ActionIntentKind::RunApp {
            caps.push("run_app".to_string());
        }
        if intent == ActionIntentKind::Gpu {
            caps.push("gpu".to_string());
        }
        if intent == ActionIntentKind::Harness {
            caps.push("harness".to_string());
        }
        if action
            .required_resources
            .iter()
            .any(|resource| matches!(resource, ResourceRequirement::Gpu { .. }))
        {
            caps.push("gpu".to_string());
        }
        if intent == ActionIntentKind::Network {
            caps.push("network".to_string());
        }
        if intent == ActionIntentKind::GitMutation {
            caps.push("modify_git".to_string());
        }
        caps.sort();
        caps.dedup();
        caps
    }
}

impl ScopedPaths {
    pub(super) fn allows(&self, path: &Path) -> bool {
        let value = normalize_path_for_scope(path);
        if self
            .deny
            .iter()
            .any(|pattern| scope_matches(pattern, &value))
        {
            return false;
        }
        self.allow.is_empty()
            || self
                .allow
                .iter()
                .any(|pattern| scope_matches(pattern, &value))
    }
}
