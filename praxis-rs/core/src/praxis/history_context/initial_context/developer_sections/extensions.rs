use praxis_features::Feature;

use crate::apps::render_apps_section;
use crate::commit_attribution::commit_message_trailer_instruction;
use crate::connectors;
use crate::plugins::render_plugins_section;
use crate::praxis::Session;
use crate::praxis::TurnContext;
use crate::render_skills_section;

pub(super) async fn push_apps_section(
    session: &Session,
    sections: &mut Vec<String>,
    turn_context: &TurnContext,
) {
    if turn_context.apps_enabled() {
        let mcp_connection_manager = session.services.mcp_connection_manager.read().await;
        let accessible_and_enabled_connectors =
            connectors::list_accessible_and_enabled_connectors_from_manager(
                &mcp_connection_manager,
                &turn_context.config,
            )
            .await;
        if let Some(apps_section) = render_apps_section(&accessible_and_enabled_connectors) {
            sections.push(apps_section);
        }
    }
}

pub(super) fn push_skills_section(sections: &mut Vec<String>, turn_context: &TurnContext) {
    let implicit_skills = turn_context
        .turn_skills
        .outcome
        .allowed_skills_for_implicit_invocation();
    if let Some(skills_section) = render_skills_section(&implicit_skills) {
        sections.push(skills_section);
    }
}

pub(super) fn push_plugins_section(
    session: &Session,
    sections: &mut Vec<String>,
    turn_context: &TurnContext,
) {
    let loaded_plugins = session
        .services
        .plugins_manager
        .plugins_for_config(&turn_context.config);
    if let Some(plugin_section) = render_plugins_section(loaded_plugins.capability_summaries()) {
        sections.push(plugin_section);
    }
}

pub(super) fn push_commit_attribution(sections: &mut Vec<String>, turn_context: &TurnContext) {
    if turn_context.features.enabled(Feature::PraxisGitCommit)
        && let Some(commit_message_instruction) =
            commit_message_trailer_instruction(turn_context.config.commit_attribution.as_deref())
    {
        sections.push(commit_message_instruction);
    }
}
