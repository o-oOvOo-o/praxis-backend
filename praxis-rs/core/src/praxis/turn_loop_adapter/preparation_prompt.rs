//! Turn preparation, prompt translation, and history projection.

#![allow(unused_imports)]

use super::*;

pub(in crate::praxis::turn_loop_adapter) mod prepare_phase {
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
                resolve_skill_dependencies_for_turn(sess, turn_context, &env_var_dependencies)
                    .await;
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
            pub(in crate::praxis::turn_loop_adapter) mentioned_plugins:
                Vec<PluginCapabilitySummary>,
            pub(in crate::praxis::turn_loop_adapter) mcp_tools: HashMap<String, ToolInfo>,
            pub(in crate::praxis::turn_loop_adapter) available_connectors: Vec<connectors::AppInfo>,
            pub(in crate::praxis::turn_loop_adapter) skill_name_counts_lower:
                HashMap<String, usize>,
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
            pub(in crate::praxis::turn_loop_adapter) explicitly_enabled_connectors:
                &'a HashSet<String>,
            pub(in crate::praxis::turn_loop_adapter) available_connectors:
                &'a [connectors::AppInfo],
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
            let user_prompt_submit_outcome = run_user_prompt_submit_hooks(
                sess,
                turn_context,
                UserMessageItem::new(input).message(),
            )
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

        let mentions =
            resolve_prepare_mentions(sess, turn_context, input, cancellation_token).await?;
        let config = turn_context.config.clone();
        resolve_prepare_dependencies(sess, turn_context, &config, &mentions, cancellation_token)
            .await;
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
}

pub(in crate::praxis::turn_loop_adapter) mod prompt_bridge {
    use praxis_protocol::models::ResponseInputItem;
    use praxis_protocol::models::ResponseItem;
    use praxis_protocol::user_input::UserInput;

    use super::super::Session;
    use super::super::TurnContext;

    mod image_content {
        use std::path::PathBuf;

        use praxis_protocol::models::ContentItem;
        use praxis_protocol::models::ResponseInputItem;
        use praxis_protocol::models::image_close_tag_text;
        use praxis_protocol::models::image_open_tag_text;
        use praxis_protocol::user_input::UserInput;

        pub(in crate::praxis::turn_loop_adapter) fn image_url_content_items(
            image_url: String,
        ) -> Vec<ContentItem> {
            vec![
                ContentItem::InputText {
                    text: image_open_tag_text(),
                },
                ContentItem::InputImage { image_url },
                ContentItem::InputText {
                    text: image_close_tag_text(),
                },
            ]
        }

        pub(in crate::praxis::turn_loop_adapter) fn local_image_prompt_content_items(
            path: &str,
        ) -> Vec<ContentItem> {
            let path = PathBuf::from(path);
            match ResponseInputItem::from(vec![UserInput::LocalImage { path }]) {
                ResponseInputItem::Message { content, .. } => content,
                _ => Vec::new(),
            }
        }
    }
    mod message_buffer {
        use praxis_protocol::models::ContentItem;
        use praxis_protocol::models::ResponseItem;

        pub(in crate::praxis::turn_loop_adapter) struct ResponseMessageBuffer {
            pending_role: Option<String>,
            pending_content: Vec<ContentItem>,
            items: Vec<ResponseItem>,
        }

        impl ResponseMessageBuffer {
            pub(in crate::praxis::turn_loop_adapter) fn new() -> Self {
                Self {
                    pending_role: None,
                    pending_content: Vec::new(),
                    items: Vec::new(),
                }
            }

            pub(in crate::praxis::turn_loop_adapter) fn push_content(
                &mut self,
                role: &str,
                content: ContentItem,
            ) {
                if self
                    .pending_role
                    .as_deref()
                    .is_some_and(|pending| pending != role)
                {
                    self.flush();
                }
                if self.pending_role.is_none() {
                    self.pending_role = Some(role.to_string());
                }
                self.pending_content.push(content);
            }

            pub(in crate::praxis::turn_loop_adapter) fn push_item(&mut self, item: ResponseItem) {
                self.flush();
                self.items.push(item);
            }

            pub(in crate::praxis::turn_loop_adapter) fn finish(mut self) -> Vec<ResponseItem> {
                self.flush();
                self.items
            }

            fn flush(&mut self) {
                let Some(role) = self.pending_role.take() else {
                    return;
                };
                if self.pending_content.is_empty() {
                    return;
                }
                self.items.push(ResponseItem::Message {
                    id: None,
                    role,
                    content: std::mem::take(&mut self.pending_content),
                    end_turn: None,
                    phase: None,
                });
            }
        }
    }
    mod message_decoder {
        use praxis_protocol::models::ContentItem;
        use praxis_protocol::models::is_image_close_tag_text;
        use praxis_protocol::models::is_image_open_tag_text;

        use super::prompt_text_decoder;

        pub(in crate::praxis::turn_loop_adapter) fn prompt_items_from_message(
            role: &str,
            content: &[ContentItem],
        ) -> Vec<praxis_loop::model::PromptItem> {
            let mut prompt_items = Vec::new();
            for item in content {
                message_content_projection(role, item).append_to(&mut prompt_items);
            }
            prompt_items
        }

        enum MessageContentProjection {
            Include(praxis_loop::model::PromptItem),
            WrapperOnly,
        }

        impl MessageContentProjection {
            fn append_to(self, prompt_items: &mut Vec<praxis_loop::model::PromptItem>) {
                match self {
                    Self::Include(item) => prompt_items.push(item),
                    Self::WrapperOnly => {}
                }
            }
        }

        fn message_content_projection(role: &str, item: &ContentItem) -> MessageContentProjection {
            match item {
                ContentItem::InputText { text } | ContentItem::OutputText { text } => {
                    if is_image_open_tag_text(text) || is_image_close_tag_text(text) {
                        MessageContentProjection::WrapperOnly
                    } else {
                        MessageContentProjection::Include(
                            prompt_text_decoder::prompt_text_item_from_role(role, text.clone()),
                        )
                    }
                }
                ContentItem::InputImage { image_url } => MessageContentProjection::Include(
                    praxis_loop::model::PromptItem::ImageUrl(image_url.clone()),
                ),
            }
        }
    }
    mod opaque {
        use praxis_protocol::models::ResponseItem;

        use super::OPAQUE_RESPONSE_ITEM_FORMAT;

        pub(in crate::praxis::turn_loop_adapter) enum OpaquePromptItemProjection {
            Opaque(praxis_loop::model::PromptItem),
            DecodeResponseItem,
        }

        pub(in crate::praxis::turn_loop_adapter) enum OpaqueResponseItemProjection {
            Restored(ResponseItem),
            NotOpaque,
            InvalidOpaque,
        }

        pub(in crate::praxis::turn_loop_adapter) fn opaque_prompt_item_projection(
            item: &ResponseItem,
        ) -> OpaquePromptItemProjection {
            serde_json::to_string(item).map_or(
                OpaquePromptItemProjection::DecodeResponseItem,
                |data| {
                    OpaquePromptItemProjection::Opaque(praxis_loop::model::PromptItem::Opaque {
                        format: OPAQUE_RESPONSE_ITEM_FORMAT.to_string(),
                        data,
                    })
                },
            )
        }

        pub(in crate::praxis::turn_loop_adapter) fn response_item_projection_from_opaque_prompt_item(
            format: &str,
            data: &str,
        ) -> OpaqueResponseItemProjection {
            if format != OPAQUE_RESPONSE_ITEM_FORMAT {
                return OpaqueResponseItemProjection::NotOpaque;
            }
            serde_json::from_str::<ResponseItem>(data).map_or(
                OpaqueResponseItemProjection::InvalidOpaque,
                OpaqueResponseItemProjection::Restored,
            )
        }
    }
    mod prompt_image_encoder {
        use super::image_content::image_url_content_items;
        use super::image_content::local_image_prompt_content_items;
        use super::message_buffer::ResponseMessageBuffer;

        pub(in crate::praxis::turn_loop_adapter) fn push_image_url(
            buffer: &mut ResponseMessageBuffer,
            image_url: &str,
        ) {
            for content in image_url_content_items(image_url.to_owned()) {
                buffer.push_content("user", content);
            }
        }

        pub(in crate::praxis::turn_loop_adapter) fn push_local_image_path(
            buffer: &mut ResponseMessageBuffer,
            path: &str,
        ) {
            for content in local_image_prompt_content_items(path) {
                buffer.push_content("user", content);
            }
        }
    }
    mod prompt_item_encoder {
        use super::message_buffer::ResponseMessageBuffer;
        use super::opaque;
        use super::opaque::OpaqueResponseItemProjection;
        use super::prompt_image_encoder;
        use super::prompt_text_encoder;
        use super::prompt_tool_encoder;

        pub(in crate::praxis::turn_loop_adapter) fn push_prompt_item(
            buffer: &mut ResponseMessageBuffer,
            item: &praxis_loop::model::PromptItem,
        ) {
            match item {
                praxis_loop::model::PromptItem::SystemText(text) => {
                    prompt_text_encoder::push_system_text(buffer, text);
                }
                praxis_loop::model::PromptItem::UserText(text) => {
                    prompt_text_encoder::push_user_text(buffer, text);
                }
                praxis_loop::model::PromptItem::AssistantText(text) => {
                    prompt_text_encoder::push_assistant_text(buffer, text);
                }
                praxis_loop::model::PromptItem::ImageUrl(image_url) => {
                    prompt_image_encoder::push_image_url(buffer, image_url);
                }
                praxis_loop::model::PromptItem::LocalImagePath(path) => {
                    prompt_image_encoder::push_local_image_path(buffer, path);
                }
                praxis_loop::model::PromptItem::ToolCall {
                    call_id,
                    name,
                    arguments,
                } => prompt_tool_encoder::push_tool_call(buffer, call_id, name, arguments),
                praxis_loop::model::PromptItem::ToolResult {
                    call_id,
                    content,
                    status,
                } => prompt_tool_encoder::push_tool_result(
                    buffer,
                    call_id,
                    content,
                    status.is_error(),
                ),
                praxis_loop::model::PromptItem::Opaque { format, data } => {
                    match opaque::response_item_projection_from_opaque_prompt_item(format, data) {
                        OpaqueResponseItemProjection::Restored(item) => buffer.push_item(item),
                        OpaqueResponseItemProjection::NotOpaque
                        | OpaqueResponseItemProjection::InvalidOpaque => {}
                    }
                }
                praxis_loop::model::PromptItem::Skill { .. }
                | praxis_loop::model::PromptItem::Mention { .. } => {}
            }
        }
    }
    mod prompt_text_decoder {
        pub(in crate::praxis::turn_loop_adapter) fn prompt_text_item_from_role(
            role: &str,
            text: String,
        ) -> praxis_loop::model::PromptItem {
            match role {
                "user" => praxis_loop::model::PromptItem::UserText(text),
                "assistant" => praxis_loop::model::PromptItem::AssistantText(text),
                _ => praxis_loop::model::PromptItem::SystemText(text),
            }
        }
    }
    mod prompt_text_encoder {
        use praxis_protocol::models::ContentItem;

        use super::message_buffer::ResponseMessageBuffer;

        pub(in crate::praxis::turn_loop_adapter) fn push_system_text(
            buffer: &mut ResponseMessageBuffer,
            text: &str,
        ) {
            push_text(
                buffer,
                "system",
                ContentItem::InputText { text: text.into() },
            );
        }

        pub(in crate::praxis::turn_loop_adapter) fn push_user_text(
            buffer: &mut ResponseMessageBuffer,
            text: &str,
        ) {
            push_text(buffer, "user", ContentItem::InputText { text: text.into() });
        }

        pub(in crate::praxis::turn_loop_adapter) fn push_assistant_text(
            buffer: &mut ResponseMessageBuffer,
            text: &str,
        ) {
            push_text(
                buffer,
                "assistant",
                ContentItem::OutputText { text: text.into() },
            );
        }

        fn push_text(buffer: &mut ResponseMessageBuffer, role: &str, content: ContentItem) {
            buffer.push_content(role, content);
        }
    }
    mod prompt_tool_encoder {
        use praxis_protocol::models::FunctionCallOutputPayload;
        use praxis_protocol::models::ResponseItem;

        use super::message_buffer::ResponseMessageBuffer;

        pub(in crate::praxis::turn_loop_adapter) fn push_tool_call(
            buffer: &mut ResponseMessageBuffer,
            call_id: &str,
            name: &str,
            arguments: &str,
        ) {
            buffer.push_item(ResponseItem::FunctionCall {
                id: None,
                provider_metadata: None,
                name: name.to_string(),
                namespace: None,
                arguments: arguments.to_string(),
                call_id: call_id.to_string(),
            });
        }

        pub(in crate::praxis::turn_loop_adapter) fn push_tool_result(
            buffer: &mut ResponseMessageBuffer,
            call_id: &str,
            content: &str,
            is_error: bool,
        ) {
            let mut output = FunctionCallOutputPayload::from_text(content.to_string());
            output.success = Some(!is_error);
            buffer.push_item(ResponseItem::FunctionCallOutput {
                call_id: call_id.to_string(),
                output,
            });
        }
    }
    mod response_decoder {
        use praxis_protocol::models::ResponseItem;

        use super::message_decoder;
        use super::opaque;
        use super::opaque::OpaquePromptItemProjection;
        use super::tool_decoder;

        pub(in crate::praxis::turn_loop_adapter) fn prompt_items_from_response_items(
            items: &[ResponseItem],
        ) -> Vec<praxis_loop::model::PromptItem> {
            items.iter().flat_map(lossless_prompt_item).collect()
        }

        fn lossless_prompt_item(item: &ResponseItem) -> Vec<praxis_loop::model::PromptItem> {
            match opaque::opaque_prompt_item_projection(item) {
                OpaquePromptItemProjection::Opaque(opaque) => vec![opaque],
                OpaquePromptItemProjection::DecodeResponseItem => decoded_prompt_items(item),
            }
        }

        fn decoded_prompt_items(item: &ResponseItem) -> Vec<praxis_loop::model::PromptItem> {
            match item {
                ResponseItem::Message { role, content, .. } => {
                    message_decoder::prompt_items_from_message(role.as_str(), content)
                }
                _ => tool_decoder::prompt_items_from_tool_item(item),
            }
        }
    }
    mod response_encoder {
        use praxis_protocol::models::ResponseItem;

        use super::message_buffer::ResponseMessageBuffer;
        use super::prompt_item_encoder;

        pub(in crate::praxis::turn_loop_adapter) fn response_items_from_prompt_items(
            prompt_items: &[praxis_loop::model::PromptItem],
        ) -> Vec<ResponseItem> {
            let mut buffer = ResponseMessageBuffer::new();

            for item in prompt_items {
                prompt_item_encoder::push_prompt_item(&mut buffer, item);
            }

            buffer.finish()
        }
    }
    mod tool_decoder {
        use praxis_loop::tool::ToolResultStatus;
        use praxis_protocol::models::ResponseItem;

        pub(in crate::praxis::turn_loop_adapter) fn prompt_items_from_tool_item(
            item: &ResponseItem,
        ) -> Vec<praxis_loop::model::PromptItem> {
            match item {
                ResponseItem::FunctionCall {
                    name,
                    arguments,
                    call_id,
                    ..
                }
                | ResponseItem::CustomToolCall {
                    name,
                    input: arguments,
                    call_id,
                    ..
                } => vec![praxis_loop::model::PromptItem::ToolCall {
                    call_id: call_id.clone(),
                    name: name.clone(),
                    arguments: arguments.clone(),
                }],
                ResponseItem::FunctionCallOutput { call_id, output }
                | ResponseItem::CustomToolCallOutput {
                    call_id, output, ..
                } => output.body.to_text().map_or_else(Vec::new, |content| {
                    vec![praxis_loop::model::PromptItem::ToolResult {
                        call_id: call_id.clone(),
                        content,
                        status: ToolResultStatus::from_success_flag(output.success != Some(false)),
                    }]
                }),
                _ => Vec::new(),
            }
        }
    }

    pub(in crate::praxis::turn_loop_adapter) use response_decoder::prompt_items_from_response_items;
    pub(in crate::praxis::turn_loop_adapter) use response_encoder::response_items_from_prompt_items;

    pub(in crate::praxis::turn_loop_adapter) const OPAQUE_RESPONSE_ITEM_FORMAT: &str =
        "praxis.response_item.v1";

    pub(in crate::praxis::turn_loop_adapter) async fn initial_prompt_items_from_session_history(
        sess: &Session,
        turn_context: &TurnContext,
    ) -> Vec<praxis_loop::model::PromptItem> {
        let items = sess
            .clone_history()
            .await
            .for_prompt(&turn_context.model_info.input_modalities);
        prompt_items_from_response_items(&items)
    }

    pub(in crate::praxis::turn_loop_adapter) fn input_to_turn_input(
        input: &[UserInput],
    ) -> praxis_loop::TurnInput {
        if input.is_empty() {
            return praxis_loop::TurnInput::default();
        }
        let response_input_item = ResponseInputItem::from(input.to_vec());
        let response_item = ResponseItem::from(response_input_item);
        let prompt_items = match opaque::opaque_prompt_item_projection(&response_item) {
            opaque::OpaquePromptItemProjection::Opaque(item) => vec![item],
            opaque::OpaquePromptItemProjection::DecodeResponseItem => {
                prompt_items_from_response_items(std::slice::from_ref(&response_item))
            }
        };
        praxis_loop::TurnInput::from_prompt_items(prompt_items)
    }
}

pub(in crate::praxis::turn_loop_adapter) mod history_bridge {
    mod history_item {
        use praxis_protocol::models::ResponseItem;

        use super::super::tool_call_bridge::loop_tool_call_to_response_item;
        use super::history_item_builders;

        pub(in crate::praxis::turn_loop_adapter) enum HistoryItemProjection {
            Persist(ResponseItem),
            RuntimeOnly,
        }

        impl HistoryItemProjection {
            pub(in crate::praxis::turn_loop_adapter) fn append_to(
                self,
                response_items: &mut Vec<ResponseItem>,
            ) {
                match self {
                    Self::Persist(item) => response_items.push(item),
                    Self::RuntimeOnly => {}
                }
            }
        }

        pub(in crate::praxis::turn_loop_adapter) fn project_loop_turn_item(
            item: &praxis_loop::model::TurnItem,
        ) -> HistoryItemProjection {
            match item {
                praxis_loop::model::TurnItem::AssistantText { item_id, text } => {
                    HistoryItemProjection::Persist(history_item_builders::assistant_message(
                        item_id.clone(),
                        text.clone(),
                    ))
                }
                praxis_loop::model::TurnItem::Reasoning { item_id, text } => {
                    HistoryItemProjection::Persist(history_item_builders::reasoning_item(
                        item_id.clone(),
                        text.clone(),
                    ))
                }
                praxis_loop::model::TurnItem::ToolCall(call) => {
                    HistoryItemProjection::Persist(loop_tool_call_to_response_item(call))
                }
                praxis_loop::model::TurnItem::ToolStarted { .. }
                | praxis_loop::model::TurnItem::ToolProgress { .. } => {
                    HistoryItemProjection::RuntimeOnly
                }
                praxis_loop::model::TurnItem::ToolResult(result) => {
                    HistoryItemProjection::Persist(history_item_builders::tool_result_item(result))
                }
                praxis_loop::model::TurnItem::SystemText(text) => HistoryItemProjection::Persist(
                    history_item_builders::text_message("system", text),
                ),
                praxis_loop::model::TurnItem::UserText(text) => HistoryItemProjection::Persist(
                    history_item_builders::text_message("user", text),
                ),
            }
        }
    }
    mod history_item_builders {
        use praxis_protocol::models::ContentItem;
        use praxis_protocol::models::FunctionCallOutputPayload;
        use praxis_protocol::models::ReasoningItemContent;
        use praxis_protocol::models::ResponseItem;
        use uuid::Uuid;

        pub(in crate::praxis::turn_loop_adapter) fn assistant_message(
            item_id: Option<String>,
            text: String,
        ) -> ResponseItem {
            ResponseItem::Message {
                id: Some(item_id.unwrap_or_else(new_item_id)),
                role: "assistant".to_string(),
                content: vec![ContentItem::OutputText { text }],
                end_turn: None,
                phase: None,
            }
        }

        pub(in crate::praxis::turn_loop_adapter) fn reasoning_item(
            item_id: Option<String>,
            text: String,
        ) -> ResponseItem {
            ResponseItem::Reasoning {
                id: item_id.unwrap_or_else(new_item_id),
                summary: Vec::new(),
                content: Some(vec![ReasoningItemContent::ReasoningText { text }]),
                encrypted_content: None,
            }
        }

        pub(in crate::praxis::turn_loop_adapter) fn tool_result_item(
            result: &praxis_loop::tool::ToolResult,
        ) -> ResponseItem {
            let mut output = FunctionCallOutputPayload::from_text(result.content.clone());
            output.success = Some(result.is_success());
            ResponseItem::FunctionCallOutput {
                call_id: result.call_id.clone(),
                output,
            }
        }

        pub(in crate::praxis::turn_loop_adapter) fn text_message(
            role: &str,
            text: &str,
        ) -> ResponseItem {
            ResponseItem::Message {
                id: Some(new_item_id()),
                role: role.to_string(),
                content: vec![ContentItem::InputText {
                    text: text.to_string(),
                }],
                end_turn: None,
                phase: None,
            }
        }

        fn new_item_id() -> String {
            Uuid::new_v4().to_string()
        }
    }

    use history_item::project_loop_turn_item;

    pub(in crate::praxis::turn_loop_adapter) fn loop_turn_items_to_response_items(
        items: &[praxis_loop::model::TurnItem],
    ) -> Vec<praxis_protocol::models::ResponseItem> {
        let mut response_items = Vec::new();
        for item in items {
            project_loop_turn_item(item).append_to(&mut response_items);
        }
        response_items
    }
}
