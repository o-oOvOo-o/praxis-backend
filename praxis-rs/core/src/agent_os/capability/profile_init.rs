use crate::agent_os::classification::builtin_profiles;
use crate::agent_os::state::AgentOsState;

impl AgentOsState {
    pub(in crate::agent_os) fn ensure_builtin_profiles(&mut self) {
        if self.profiles.is_empty() {
            for profile in builtin_profiles() {
                self.profiles.insert(profile.profile_id.clone(), profile);
            }
        }
    }
}
