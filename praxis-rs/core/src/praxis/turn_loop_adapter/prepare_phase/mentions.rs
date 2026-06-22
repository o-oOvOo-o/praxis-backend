use std::collections::HashMap;
use std::sync::Arc;

use praxis_async_utils::OrCancelExt;
use praxis_mcp::mcp_connection_manager::ToolInfo;
use praxis_protocol::user_input::UserInput;
use tokio_util::sync::CancellationToken;

use crate::SkillMetadata;
use crate::collect_explicit_skill_mentions;
use crate::connectors;
use crate::mentions::build_connector_slug_counts;
use crate::mentions::build_skill_name_counts;
use crate::mentions::collect_explicit_plugin_mentions;
use crate::plugins::PluginCapabilitySummary;

use super::super::super::Session;
use super::super::super::TurnContext;

pub(super) struct TurnPrepareMentions {
    pub(super) mentioned_skills: Vec<SkillMetadata>,
    pub(super) mentioned_plugins: Vec<PluginCapabilitySummary>,
    pub(super) mcp_tools: HashMap<String, ToolInfo>,
    pub(super) available_connectors: Vec<connectors::AppInfo>,
    pub(super) skill_name_counts_lower: HashMap<String, usize>,
}

pub(super) async fn resolve_prepare_mentions(
    sess: &Arc<Session>,
    turn_context: &Arc<TurnContext>,
    input: &[UserInput],
    cancellation_token: &CancellationToken,
) -> Option<TurnPrepareMentions> {
    let skills_outcome = turn_context.turn_skills.outcome.as_ref();
    let loaded_plugins = sess
        .services
        .plugins_manager
        .plugins_for_config(&turn_context.config);
    let mentioned_plugins =
        collect_explicit_plugin_mentions(input, loaded_plugins.capability_summaries());
    let mcp_tools = if turn_context.apps_enabled() || !mentioned_plugins.is_empty() {
        match sess
            .services
            .mcp_connection_manager
            .read()
            .await
            .list_all_tools()
            .or_cancel(cancellation_token)
            .await
        {
            Ok(mcp_tools) => mcp_tools,
            Err(_) if turn_context.apps_enabled() => return None,
            Err(_) => HashMap::new(),
        }
    } else {
        HashMap::new()
    };
    let available_connectors = if turn_context.apps_enabled() {
        let connectors = connectors::merge_plugin_apps_with_accessible(
            loaded_plugins.effective_apps(),
            connectors::accessible_connectors_from_mcp_tools(&mcp_tools),
        );
        connectors::with_app_enabled_state(connectors, &turn_context.config)
    } else {
        Vec::new()
    };
    let connector_slug_counts = build_connector_slug_counts(&available_connectors);
    let skill_name_counts_lower =
        build_skill_name_counts(&skills_outcome.skills, &skills_outcome.disabled_paths).1;
    let mentioned_skills = collect_explicit_skill_mentions(
        input,
        &skills_outcome.skills,
        &skills_outcome.disabled_paths,
        &connector_slug_counts,
    );

    Some(TurnPrepareMentions {
        mentioned_skills,
        mentioned_plugins,
        mcp_tools,
        available_connectors,
        skill_name_counts_lower,
    })
}
