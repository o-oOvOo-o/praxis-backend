use std::sync::Arc;

use praxis_protocol::user_input::UserInput;
use tokio_util::sync::CancellationToken;

use super::super::Session;
use super::super::TurnContext;

mod connector_mentions;
mod dependency_resolution;
mod injections;
mod mentions;
mod outcome;
mod session_updates;
mod tracking;
mod user_input;

use connector_mentions::collect_explicitly_enabled_connectors_for_turn;
use dependency_resolution::resolve_prepare_dependencies;
use injections::TurnPrepareInjections;
use injections::build_prepare_injections;
use injections::combine_prepare_items;
use injections::emit_skill_warnings;
use injections::record_prepare_injections;
use mentions::resolve_prepare_mentions;
pub(super) use outcome::TurnPrepareOutcome;
use session_updates::PrepareSessionUpdate;
use session_updates::commit_prepare_session_state;
use tracking::build_prepare_tracking;

pub(super) async fn prepare_turn_before_model_request(
    sess: &Arc<Session>,
    turn_context: &Arc<TurnContext>,
    input: &[UserInput],
    cancellation_token: &CancellationToken,
) -> Option<TurnPrepareOutcome> {
    sess.record_context_updates_and_set_reference_context_item(turn_context.as_ref())
        .await;

    let mentions = resolve_prepare_mentions(sess, turn_context, input, cancellation_token).await?;
    let config = turn_context.config.clone();
    resolve_prepare_dependencies(sess, turn_context, &config, &mentions, cancellation_token).await;
    let tracking = build_prepare_tracking(sess, turn_context);
    let TurnPrepareInjections {
        skill_items,
        skill_warnings,
        plugin_items,
        mentioned_plugin_metadata,
    } = build_prepare_injections(
        sess,
        turn_context,
        &mentions.mentioned_skills,
        &mentions.mentioned_plugins,
        &mentions.mcp_tools,
        &mentions.available_connectors,
        &tracking,
    )
    .await;

    emit_skill_warnings(sess, turn_context, skill_warnings).await;

    let explicitly_enabled_connectors = collect_explicitly_enabled_connectors_for_turn(
        input,
        &skill_items,
        &mentions.available_connectors,
        &mentions.skill_name_counts_lower,
    );

    commit_prepare_session_state(
        sess,
        turn_context,
        PrepareSessionUpdate {
            input,
            explicitly_enabled_connectors: &explicitly_enabled_connectors,
            available_connectors: &mentions.available_connectors,
            tracking: &tracking,
            mentioned_plugin_metadata,
            cancellation_token,
        },
    )
    .await?;

    let prepared_items = combine_prepare_items(&skill_items, &plugin_items);
    record_prepare_injections(sess, turn_context, &skill_items, &plugin_items).await;
    Some(TurnPrepareOutcome {
        explicitly_enabled_connectors,
        prepared_items,
    })
}
