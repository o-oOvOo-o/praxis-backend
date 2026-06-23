use crate::environment_context::EnvironmentContext;
use crate::instructions::UserInstructions;
use crate::praxis::Session;
use crate::praxis::TurnContext;

pub(super) async fn build_contextual_user_sections(
    session: &Session,
    turn_context: &TurnContext,
) -> Vec<String> {
    let mut sections = Vec::<String>::with_capacity(2);
    if let Some(user_instructions) = turn_context.user_instructions.as_deref() {
        sections.push(
            UserInstructions {
                text: user_instructions.to_string(),
                directory: turn_context.cwd.to_string_lossy().into_owned(),
            }
            .serialize_to_text(),
        );
    }

    let subagents = session
        .services
        .agent_control
        .format_environment_context_subagents(session.conversation_id)
        .await;
    sections.push(
        EnvironmentContext::from_turn_context(turn_context, session.user_shell().as_ref())
            .with_subagents(subagents)
            .serialize_to_xml(),
    );
    sections
}
