use crate::agent_os::classification::requires_compile;
use crate::agent_os::classification::requires_cpu_heavy;
use crate::agent_os::classification::requires_write;
use crate::agent_os::records::ActionIntent;
use crate::agent_os::records::ActionIntentKind;
use crate::agent_os::records::CapabilityProfile;
use crate::agent_os::records::ResourceRequirement;

impl CapabilityProfile {
    pub(in crate::agent_os) fn capability_names_for_action(
        &self,
        action: &ActionIntent,
    ) -> Vec<String> {
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
