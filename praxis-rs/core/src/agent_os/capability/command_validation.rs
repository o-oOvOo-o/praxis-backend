use std::path::Path;

use crate::agent_os::classification::denylist_surface;
use crate::agent_os::classification::requires_compile;
use crate::agent_os::classification::requires_cpu_heavy;
use crate::agent_os::classification::requires_write;
use crate::agent_os::records::ActionIntent;
use crate::agent_os::records::ActionIntentKind;
use crate::agent_os::records::CapabilityProfile;
use crate::agent_os::records::ResourceRequirement;

impl CapabilityProfile {
    pub(in crate::agent_os) fn validate_command_intent(
        &self,
        intent: &ActionIntent,
        command: &[String],
        cwd: &Path,
    ) -> Result<(), String> {
        if command_denies(self, command) {
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
}

fn command_denies(profile: &CapabilityProfile, command: &[String]) -> bool {
    let rendered = denylist_surface(command).to_ascii_lowercase();
    profile
        .command_denylist
        .iter()
        .any(|pattern| rendered.contains(&pattern.to_ascii_lowercase()))
}
