mod extensions;
mod permissions;
mod state_updates;
mod static_instructions;

use crate::praxis::Session;
use crate::praxis::TurnContext;

use super::state_snapshot::InitialContextStateSnapshot;
use extensions::push_apps_section;
use extensions::push_commit_attribution;
use extensions::push_plugins_section;
use extensions::push_skills_section;
use permissions::push_permission_policy;
use state_updates::push_model_update;
use state_updates::push_realtime_update;
use static_instructions::push_collaboration_mode;
use static_instructions::push_developer_instructions;
use static_instructions::push_memory_prompt;
use static_instructions::push_personality_override;

pub(super) async fn build_developer_sections(
    session: &Session,
    turn_context: &TurnContext,
    snapshot: &InitialContextStateSnapshot,
    separate_guardian_developer_message: bool,
) -> Vec<String> {
    let mut sections = Vec::<String>::with_capacity(8);
    push_model_update(&mut sections, turn_context, snapshot);
    push_permission_policy(session, &mut sections, turn_context);
    push_developer_instructions(
        &mut sections,
        turn_context,
        separate_guardian_developer_message,
    );
    push_memory_prompt(&mut sections, turn_context).await;
    push_collaboration_mode(&mut sections, snapshot);
    push_realtime_update(&mut sections, turn_context, snapshot);
    push_personality_override(session, &mut sections, turn_context, snapshot);
    push_apps_section(session, &mut sections, turn_context).await;
    push_skills_section(&mut sections, turn_context);
    push_plugins_section(session, &mut sections, turn_context);
    push_commit_attribution(&mut sections, turn_context);
    sections
}
