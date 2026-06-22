use std::collections::HashMap;
use std::collections::HashSet;
use std::sync::Arc;

use praxis_analytics::AppInvocation;
use praxis_analytics::InvocationType;
use praxis_analytics::TrackEventsContext;
use praxis_plugin::PluginTelemetryMetadata;
use praxis_protocol::models::ResponseItem;
use praxis_protocol::user_input::UserInput;

use crate::connectors;
use crate::mentions::collect_explicit_app_ids;

use super::super::super::Session;
use super::super::super::model_request::collect_explicit_app_ids_from_skill_items;

pub(super) fn collect_explicitly_enabled_connectors_for_turn(
    input: &[UserInput],
    skill_items: &[ResponseItem],
    available_connectors: &[connectors::AppInfo],
    skill_name_counts_lower: &HashMap<String, usize>,
) -> HashSet<String> {
    let mut connector_ids = collect_explicit_app_ids(input);
    connector_ids.extend(collect_explicit_app_ids_from_skill_items(
        skill_items,
        available_connectors,
        skill_name_counts_lower,
    ));
    connector_ids
}

pub(super) fn collect_mentioned_app_invocations(
    available_connectors: &[connectors::AppInfo],
    explicitly_enabled_connectors: &HashSet<String>,
) -> Vec<AppInvocation> {
    let connector_names_by_id = available_connectors
        .iter()
        .map(|connector| (connector.id.as_str(), connector.name.as_str()))
        .collect::<HashMap<&str, &str>>();
    explicitly_enabled_connectors
        .iter()
        .map(|connector_id| AppInvocation {
            connector_id: Some(connector_id.clone()),
            app_name: connector_names_by_id
                .get(connector_id.as_str())
                .map(|name| (*name).to_string()),
            invocation_type: Some(InvocationType::Explicit),
        })
        .collect()
}

pub(super) fn track_prepare_mentions(
    sess: &Arc<Session>,
    tracking: &TrackEventsContext,
    mentioned_app_invocations: Vec<AppInvocation>,
    mentioned_plugin_metadata: Vec<PluginTelemetryMetadata>,
) {
    sess.services
        .analytics_events_client
        .track_app_mentioned(tracking.clone(), mentioned_app_invocations);
    for plugin in mentioned_plugin_metadata {
        sess.services
            .analytics_events_client
            .track_plugin_used(tracking.clone(), plugin);
    }
}
