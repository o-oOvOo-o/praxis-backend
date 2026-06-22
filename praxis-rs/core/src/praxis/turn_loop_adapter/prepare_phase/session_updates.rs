use std::collections::HashSet;
use std::sync::Arc;

use praxis_analytics::TrackEventsContext;
use praxis_plugin::PluginTelemetryMetadata;
use praxis_protocol::user_input::UserInput;
use tokio_util::sync::CancellationToken;

use crate::connectors;
use crate::hook_runtime::record_additional_contexts;
use crate::hook_runtime::run_pending_session_start_hooks;

use super::super::super::PreviousTurnSettings;
use super::super::super::Session;
use super::super::super::TurnContext;
use super::connector_mentions::collect_mentioned_app_invocations;
use super::connector_mentions::track_prepare_mentions;
use super::user_input::record_user_input_and_collect_additional_contexts;

pub(super) struct PrepareSessionUpdate<'a> {
    pub(super) input: &'a [UserInput],
    pub(super) explicitly_enabled_connectors: &'a HashSet<String>,
    pub(super) available_connectors: &'a [connectors::AppInfo],
    pub(super) tracking: &'a TrackEventsContext,
    pub(super) mentioned_plugin_metadata: Vec<PluginTelemetryMetadata>,
    pub(super) cancellation_token: &'a CancellationToken,
}

pub(super) async fn commit_prepare_session_state(
    sess: &Arc<Session>,
    turn_context: &Arc<TurnContext>,
    update: PrepareSessionUpdate<'_>,
) -> Option<()> {
    let mentioned_app_invocations = collect_mentioned_app_invocations(
        update.available_connectors,
        update.explicitly_enabled_connectors,
    );

    if run_pending_session_start_hooks(sess, turn_context).await {
        return None;
    }

    let additional_contexts =
        record_user_input_and_collect_additional_contexts(sess, turn_context, update.input).await?;

    track_prepare_mentions(
        sess,
        update.tracking,
        mentioned_app_invocations,
        update.mentioned_plugin_metadata,
    );
    sess.merge_connector_selection(update.explicitly_enabled_connectors.clone())
        .await;
    record_additional_contexts(sess, turn_context, additional_contexts).await;

    if !update.input.is_empty() {
        sess.set_previous_turn_settings(Some(PreviousTurnSettings {
            model: turn_context.model_info.slug.clone(),
            realtime_active: Some(turn_context.realtime_active),
        }))
        .await;
    }

    sess.maybe_start_ghost_snapshot(
        Arc::clone(turn_context),
        update.cancellation_token.child_token(),
    )
    .await;
    Some(())
}
