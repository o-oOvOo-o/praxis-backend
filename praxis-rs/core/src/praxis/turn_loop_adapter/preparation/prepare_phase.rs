use std::sync::Arc;

use praxis_protocol::user_input::UserInput;
use tokio_util::sync::CancellationToken;

use super::super::Session;
use super::super::TurnContext;

mod connector_mentions {

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

    pub(in crate::praxis::turn_loop_adapter) fn collect_explicitly_enabled_connectors_for_turn(
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

    pub(in crate::praxis::turn_loop_adapter) fn collect_mentioned_app_invocations(
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

    pub(in crate::praxis::turn_loop_adapter) fn track_prepare_mentions(
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
}
mod dependency_resolution {
    use std::sync::Arc;

    use praxis_features::Feature;
    use tokio_util::sync::CancellationToken;

    use crate::collect_env_var_dependencies;
    use crate::config::Config;
    use crate::mcp_skill_dependencies::maybe_prompt_and_install_mcp_dependencies;
    use crate::resolve_skill_dependencies_for_turn;

    use super::super::super::Session;
    use super::super::super::TurnContext;
    use super::mentions::TurnPrepareMentions;

    pub(in crate::praxis::turn_loop_adapter) async fn resolve_prepare_dependencies(
        sess: &Arc<Session>,
        turn_context: &Arc<TurnContext>,
        config: &Config,
        mentions: &TurnPrepareMentions,
        cancellation_token: &CancellationToken,
    ) {
        if config
            .features
            .enabled(Feature::SkillEnvVarDependencyPrompt)
        {
            let env_var_dependencies = collect_env_var_dependencies(&mentions.mentioned_skills);
            resolve_skill_dependencies_for_turn(sess, turn_context, &env_var_dependencies).await;
        }

        maybe_prompt_and_install_mcp_dependencies(
            sess.as_ref(),
            turn_context.as_ref(),
            cancellation_token,
            &mentions.mentioned_skills,
        )
        .await;
    }
}
mod injections {
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

    pub(in crate::praxis::turn_loop_adapter) struct TurnPrepareInjections {
        pub(in crate::praxis::turn_loop_adapter) skill_items: Vec<ResponseItem>,
        pub(in crate::praxis::turn_loop_adapter) skill_warnings: Vec<String>,
        pub(in crate::praxis::turn_loop_adapter) plugin_items: Vec<ResponseItem>,
        pub(in crate::praxis::turn_loop_adapter) mentioned_plugin_metadata:
            Vec<PluginTelemetryMetadata>,
    }

    pub(in crate::praxis::turn_loop_adapter) async fn build_prepare_injections(
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
        let plugin_items =
            build_plugin_injections(mentioned_plugins, mcp_tools, available_connectors);
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

    pub(in crate::praxis::turn_loop_adapter) async fn emit_skill_warnings(
        sess: &Arc<Session>,
        turn_context: &Arc<TurnContext>,
        warnings: Vec<String>,
    ) {
        for message in warnings {
            sess.turn_event_emitter(turn_context).warning(message).await;
        }
    }

    pub(in crate::praxis::turn_loop_adapter) async fn record_prepare_injections(
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

    pub(in crate::praxis::turn_loop_adapter) fn combine_prepare_items(
        skill_items: &[ResponseItem],
        plugin_items: &[ResponseItem],
    ) -> Vec<ResponseItem> {
        skill_items
            .iter()
            .chain(plugin_items.iter())
            .cloned()
            .collect()
    }
}
mod mentions {
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

    pub(in crate::praxis::turn_loop_adapter) struct TurnPrepareMentions {
        pub(in crate::praxis::turn_loop_adapter) mentioned_skills: Vec<SkillMetadata>,
        pub(in crate::praxis::turn_loop_adapter) mentioned_plugins: Vec<PluginCapabilitySummary>,
        pub(in crate::praxis::turn_loop_adapter) mcp_tools: HashMap<String, ToolInfo>,
        pub(in crate::praxis::turn_loop_adapter) available_connectors: Vec<connectors::AppInfo>,
        pub(in crate::praxis::turn_loop_adapter) skill_name_counts_lower: HashMap<String, usize>,
    }

    pub(in crate::praxis::turn_loop_adapter) async fn resolve_prepare_mentions(
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
}
mod outcome {
    use std::collections::HashSet;

    use praxis_protocol::models::ResponseItem;

    #[derive(Debug)]
    pub(in crate::praxis::turn_loop_adapter) struct TurnPrepareOutcome {
        pub(in crate::praxis::turn_loop_adapter) explicitly_enabled_connectors: HashSet<String>,
        pub(in crate::praxis::turn_loop_adapter) prepared_items: Vec<ResponseItem>,
    }
}
mod session_updates {
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

    pub(in crate::praxis::turn_loop_adapter) struct PrepareSessionUpdate<'a> {
        pub(in crate::praxis::turn_loop_adapter) input: &'a [UserInput],
        pub(in crate::praxis::turn_loop_adapter) explicitly_enabled_connectors: &'a HashSet<String>,
        pub(in crate::praxis::turn_loop_adapter) available_connectors: &'a [connectors::AppInfo],
        pub(in crate::praxis::turn_loop_adapter) tracking: &'a TrackEventsContext,
        pub(in crate::praxis::turn_loop_adapter) mentioned_plugin_metadata:
            Vec<PluginTelemetryMetadata>,
        pub(in crate::praxis::turn_loop_adapter) cancellation_token: &'a CancellationToken,
    }

    pub(in crate::praxis::turn_loop_adapter) async fn commit_prepare_session_state(
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
            record_user_input_and_collect_additional_contexts(sess, turn_context, update.input)
                .await?;

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

        sess.maybe_start_workspace_checkpoint(
            Arc::clone(turn_context),
            update.cancellation_token.child_token(),
        )
        .await;
        Some(())
    }
}
mod tracking {
    use std::sync::Arc;

    use praxis_analytics::TrackEventsContext;
    use praxis_analytics::build_track_events_context;

    use super::super::super::Session;
    use super::super::super::TurnContext;

    pub(in crate::praxis::turn_loop_adapter) fn build_prepare_tracking(
        sess: &Arc<Session>,
        turn_context: &Arc<TurnContext>,
    ) -> TrackEventsContext {
        build_track_events_context(
            turn_context.model_info.slug.clone(),
            sess.conversation_id.to_string(),
            turn_context.sub_id.clone(),
        )
    }
}
mod user_input {
    use std::sync::Arc;

    use praxis_protocol::items::UserMessageItem;
    use praxis_protocol::models::ResponseInputItem;
    use praxis_protocol::models::ResponseItem;
    use praxis_protocol::user_input::UserInput;

    use crate::hook_runtime::record_additional_contexts;
    use crate::hook_runtime::run_user_prompt_submit_hooks;

    use super::super::super::Session;
    use super::super::super::TurnContext;

    pub(in crate::praxis::turn_loop_adapter) async fn record_user_input_and_collect_additional_contexts(
        sess: &Arc<Session>,
        turn_context: &Arc<TurnContext>,
        input: &[UserInput],
    ) -> Option<Vec<String>> {
        if input.is_empty() {
            return Some(Vec::new());
        }

        let initial_input_for_turn: ResponseInputItem = ResponseInputItem::from(input.to_vec());
        let response_item: ResponseItem = initial_input_for_turn.clone().into();
        let user_prompt_submit_outcome =
            run_user_prompt_submit_hooks(sess, turn_context, UserMessageItem::new(input).message())
                .await;
        if user_prompt_submit_outcome.should_stop {
            record_additional_contexts(
                sess,
                turn_context,
                user_prompt_submit_outcome.additional_contexts,
            )
            .await;
            return None;
        }

        sess.record_user_prompt_and_emit_turn_item(
            turn_context.as_ref(),
            input,
            &response_item,
            None,
        )
        .await;
        Some(user_prompt_submit_outcome.additional_contexts)
    }
}
use connector_mentions::collect_explicitly_enabled_connectors_for_turn;
use dependency_resolution::resolve_prepare_dependencies;
use injections::TurnPrepareInjections;
use injections::build_prepare_injections;
use injections::combine_prepare_items;
use injections::emit_skill_warnings;
use injections::record_prepare_injections;
use mentions::resolve_prepare_mentions;
pub(in crate::praxis::turn_loop_adapter) use outcome::TurnPrepareOutcome;
use session_updates::PrepareSessionUpdate;
use session_updates::commit_prepare_session_state;
use tracking::build_prepare_tracking;

pub(in crate::praxis::turn_loop_adapter) async fn prepare_turn_before_model_request(
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
