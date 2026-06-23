use praxis_features::Feature;
use praxis_protocol::models::DeveloperInstructions;

use crate::memories::prompts::build_memory_tool_developer_instructions;
use crate::praxis::Session;
use crate::praxis::TurnContext;

use super::super::state_snapshot::InitialContextStateSnapshot;

pub(super) fn push_developer_instructions(
    sections: &mut Vec<String>,
    turn_context: &TurnContext,
    separate_guardian_developer_message: bool,
) {
    if !separate_guardian_developer_message
        && let Some(developer_instructions) = turn_context.developer_instructions.as_deref()
    {
        sections.push(developer_instructions.to_string());
    }
}

pub(super) async fn push_memory_prompt(sections: &mut Vec<String>, turn_context: &TurnContext) {
    if turn_context.features.enabled(Feature::MemoryTool)
        && turn_context.config.memories.use_memories
        && let Some(memory_prompt) =
            build_memory_tool_developer_instructions(&turn_context.config.praxis_home).await
    {
        sections.push(memory_prompt);
    }
}

pub(super) fn push_collaboration_mode(
    sections: &mut Vec<String>,
    snapshot: &InitialContextStateSnapshot,
) {
    if let Some(collab_instructions) =
        DeveloperInstructions::from_collaboration_mode(&snapshot.collaboration_mode)
    {
        sections.push(collab_instructions.into_text());
    }
}

pub(super) fn push_personality_override(
    session: &Session,
    sections: &mut Vec<String>,
    turn_context: &TurnContext,
    snapshot: &InitialContextStateSnapshot,
) {
    if session.features.enabled(Feature::Personality)
        && let Some(personality) = turn_context.personality
    {
        let model_info = turn_context.model_info.clone();
        let has_baked_personality = model_info.supports_personality()
            && snapshot.base_instructions == model_info.get_model_instructions(Some(personality));
        if !has_baked_personality
            && let Some(personality_message) =
                crate::context_manager::updates::personality_message_for(&model_info, personality)
        {
            sections.push(
                DeveloperInstructions::personality_spec_message(personality_message).into_text(),
            );
        }
    }
}
