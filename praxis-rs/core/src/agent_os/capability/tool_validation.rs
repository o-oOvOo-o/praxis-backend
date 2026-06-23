use crate::agent_os::records::ActionIntent;
use crate::agent_os::records::CapabilityProfile;
use crate::agent_os::records::ResourceRequirement;

impl CapabilityProfile {
    pub(in crate::agent_os) fn validate_tool_intent(
        &self,
        intent: &ActionIntent,
    ) -> Result<(), String> {
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
}
