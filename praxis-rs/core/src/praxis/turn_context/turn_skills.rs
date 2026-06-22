use std::sync::Arc;

use crate::SkillLoadOutcome;
use crate::config::Config;
use crate::skills_load_input_from_config;

use super::super::Session;

pub(super) fn load(session: &Session, per_turn_config: &Config) -> Arc<SkillLoadOutcome> {
    let plugin_outcome = session
        .services
        .plugins_manager
        .plugins_for_config(per_turn_config);
    let effective_skill_roots = plugin_outcome.effective_skill_roots();
    let skills_input = skills_load_input_from_config(per_turn_config, effective_skill_roots);
    Arc::new(
        session
            .services
            .skills_manager
            .skills_for_config(&skills_input),
    )
}
