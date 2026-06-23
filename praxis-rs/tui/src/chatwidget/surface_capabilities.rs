use super::*;

impl ChatWidget {
    pub(super) fn realtime_conversation_enabled(&self) -> bool {
        self.config.features.enabled(Feature::RealtimeConversation)
            && cfg!(not(target_os = "linux"))
    }

    pub(super) fn realtime_audio_device_selection_enabled(&self) -> bool {
        self.realtime_conversation_enabled()
    }

    pub(super) fn set_skills(&mut self, skills: Option<Vec<SkillMetadata>>) {
        self.bottom_pane.set_skills(skills);
    }
}
