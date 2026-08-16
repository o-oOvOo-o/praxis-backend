use crate::capabilities::ResolvedSkillsCapability;
use crate::capabilities::publish_resolved_skills;
use crate::config::Config;
use crate::skills_load_input_from_config;

use super::super::Session;

pub(super) fn load(
    session: &Session,
    turn_id: &str,
    per_turn_config: &Config,
) -> ResolvedSkillsCapability {
    let plugin_outcome = session
        .services
        .plugins_manager
        .plugins_for_config(per_turn_config);
    let effective_skill_roots = plugin_outcome.effective_skill_roots();
    let skills_input = skills_load_input_from_config(per_turn_config, effective_skill_roots);
    let outcome = session
        .services
        .skills_manager
        .skills_for_config(&skills_input);
    publish_resolved_skills(
        &session.services._capability_scope,
        session.conversation_id,
        turn_id,
        outcome,
    )
    .expect("publish turn-scoped resolved Skills capability")
}
