use std::collections::HashMap;
use std::sync::Arc;

use praxis_analytics::TrackEventsContext;
use praxis_mcp::mcp_connection_manager::ToolInfo;
use praxis_plugin::PluginTelemetryMetadata;
use praxis_protocol::models::ResponseItem;

use crate::SkillInjections;
use crate::SkillMetadata;
use crate::build_skill_injections;
use crate::connectors;
use crate::plugins::PluginCapabilitySummary;
use crate::plugins::build_plugin_injections;

use super::super::super::Session;
use super::super::super::TurnContext;

pub(super) struct TurnPrepareInjections {
    pub(super) skill_items: Vec<ResponseItem>,
    pub(super) skill_warnings: Vec<String>,
    pub(super) plugin_items: Vec<ResponseItem>,
    pub(super) mentioned_plugin_metadata: Vec<PluginTelemetryMetadata>,
}

pub(super) async fn build_prepare_injections(
    sess: &Arc<Session>,
    turn_context: &Arc<TurnContext>,
    mentioned_skills: &[SkillMetadata],
    mentioned_plugins: &[PluginCapabilitySummary],
    mcp_tools: &HashMap<String, ToolInfo>,
    available_connectors: &[connectors::AppInfo],
    tracking: &TrackEventsContext,
) -> TurnPrepareInjections {
    let session_telemetry = turn_context.session_telemetry.clone();
    let SkillInjections {
        items: skill_items,
        warnings: skill_warnings,
    } = build_skill_injections(
        mentioned_skills,
        Some(&session_telemetry),
        &sess.services.analytics_events_client,
        tracking.clone(),
    )
    .await;
    let plugin_items = build_plugin_injections(mentioned_plugins, mcp_tools, available_connectors);
    let mentioned_plugin_metadata = mentioned_plugins
        .iter()
        .filter_map(PluginCapabilitySummary::telemetry_metadata)
        .collect::<Vec<_>>();

    TurnPrepareInjections {
        skill_items,
        skill_warnings,
        plugin_items,
        mentioned_plugin_metadata,
    }
}

pub(super) async fn emit_skill_warnings(
    sess: &Arc<Session>,
    turn_context: &Arc<TurnContext>,
    warnings: Vec<String>,
) {
    for message in warnings {
        sess.turn_event_emitter(turn_context).warning(message).await;
    }
}

pub(super) async fn record_prepare_injections(
    sess: &Arc<Session>,
    turn_context: &Arc<TurnContext>,
    skill_items: &[ResponseItem],
    plugin_items: &[ResponseItem],
) {
    if !skill_items.is_empty() {
        sess.record_conversation_items(turn_context, skill_items)
            .await;
    }
    if !plugin_items.is_empty() {
        sess.record_conversation_items(turn_context, plugin_items)
            .await;
    }
}

pub(super) fn combine_prepare_items(
    skill_items: &[ResponseItem],
    plugin_items: &[ResponseItem],
) -> Vec<ResponseItem> {
    skill_items
        .iter()
        .chain(plugin_items.iter())
        .cloned()
        .collect()
}
