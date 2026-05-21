use crate::events::AppGatewayRpcTransport;
use crate::events::PraxisAppGatewayClientMetadata;
use crate::events::PraxisAppMentionedEventRequest;
use crate::events::PraxisAppUsedEventRequest;
use crate::events::PraxisPluginEventRequest;
use crate::events::PraxisPluginUsedEventRequest;
use crate::events::PraxisRuntimeMetadata;
use crate::events::SkillInvocationEventParams;
use crate::events::SkillInvocationEventRequest;
use crate::events::ThreadInitializedEvent;
use crate::events::ThreadInitializedEventParams;
use crate::events::TrackEventRequest;
use crate::events::plugin_state_event_type;
use crate::events::praxis_app_metadata;
use crate::events::praxis_plugin_metadata;
use crate::events::praxis_plugin_used_metadata;
use crate::events::thread_source_name;
use crate::facts::AnalyticsFact;
use crate::facts::AppGatewayInitializeFact;
use crate::facts::AppMentionedInput;
use crate::facts::AppUsedInput;
use crate::facts::CustomAnalyticsFact;
use crate::facts::PluginState;
use crate::facts::PluginStateChangedInput;
use crate::facts::PluginUsedInput;
use crate::facts::SkillInvokedInput;
use crate::facts::ThreadInitializedFact;
use praxis_git_utils::collect_git_info;
use praxis_git_utils::get_git_repo_root;
use praxis_login::default_client::originator;
use praxis_protocol::protocol::SkillScope;
use sha1::Digest;
use std::collections::HashMap;
use std::path::Path;

#[derive(Default)]
pub(crate) struct AnalyticsReducer {
    connections: HashMap<u64, ConnectionState>,
}

struct ConnectionState {
    app_gateway_client: PraxisAppGatewayClientMetadata,
    runtime: PraxisRuntimeMetadata,
}

impl AnalyticsReducer {
    pub(crate) async fn ingest(&mut self, input: AnalyticsFact, out: &mut Vec<TrackEventRequest>) {
        match input {
            AnalyticsFact::Initialize {
                connection_id,
                initialize,
                product_client_id,
                runtime,
                rpc_transport,
            } => {
                self.ingest_initialize(
                    connection_id,
                    initialize,
                    product_client_id,
                    runtime,
                    rpc_transport,
                );
            }
            AnalyticsFact::ThreadInitialized {
                connection_id,
                thread,
            } => {
                self.ingest_thread_initialized(connection_id, thread, out);
            }
            AnalyticsFact::Custom(input) => match input {
                CustomAnalyticsFact::SkillInvoked(input) => {
                    self.ingest_skill_invoked(input, out).await;
                }
                CustomAnalyticsFact::AppMentioned(input) => {
                    self.ingest_app_mentioned(input, out);
                }
                CustomAnalyticsFact::AppUsed(input) => {
                    self.ingest_app_used(input, out);
                }
                CustomAnalyticsFact::PluginUsed(input) => {
                    self.ingest_plugin_used(input, out);
                }
                CustomAnalyticsFact::PluginStateChanged(input) => {
                    self.ingest_plugin_state_changed(input, out);
                }
            },
        }
    }

    fn ingest_initialize(
        &mut self,
        connection_id: u64,
        initialize: AppGatewayInitializeFact,
        product_client_id: String,
        runtime: PraxisRuntimeMetadata,
        rpc_transport: AppGatewayRpcTransport,
    ) {
        self.connections.insert(
            connection_id,
            ConnectionState {
                app_gateway_client: PraxisAppGatewayClientMetadata {
                    product_client_id,
                    client_name: Some(initialize.client_name),
                    client_version: initialize.client_version,
                    rpc_transport,
                    experimental_api_enabled: initialize.experimental_api_enabled,
                },
                runtime,
            },
        );
    }

    async fn ingest_skill_invoked(
        &mut self,
        input: SkillInvokedInput,
        out: &mut Vec<TrackEventRequest>,
    ) {
        let SkillInvokedInput {
            tracking,
            invocations,
        } = input;
        for invocation in invocations {
            let skill_scope = match invocation.skill_scope {
                SkillScope::User => "user",
                SkillScope::Repo => "repo",
                SkillScope::System => "system",
                SkillScope::Admin => "admin",
            };
            let repo_root = get_git_repo_root(invocation.skill_path.as_path());
            let repo_url = if let Some(root) = repo_root.as_ref() {
                collect_git_info(root)
                    .await
                    .and_then(|info| info.repository_url)
            } else {
                None
            };
            let skill_id = skill_id_for_local_skill(
                repo_url.as_deref(),
                repo_root.as_deref(),
                invocation.skill_path.as_path(),
                invocation.skill_name.as_str(),
            );
            out.push(TrackEventRequest::SkillInvocation(
                SkillInvocationEventRequest {
                    event_type: "skill_invocation",
                    skill_id,
                    skill_name: invocation.skill_name.clone(),
                    event_params: SkillInvocationEventParams {
                        thread_id: Some(tracking.thread_id.clone()),
                        invoke_type: Some(invocation.invocation_type),
                        model_slug: Some(tracking.model_slug.clone()),
                        product_client_id: Some(originator().value),
                        repo_url,
                        skill_scope: Some(skill_scope.to_string()),
                    },
                },
            ));
        }
    }

    fn ingest_app_mentioned(&mut self, input: AppMentionedInput, out: &mut Vec<TrackEventRequest>) {
        let AppMentionedInput { tracking, mentions } = input;
        out.extend(mentions.into_iter().map(|mention| {
            let event_params = praxis_app_metadata(&tracking, mention);
            TrackEventRequest::AppMentioned(PraxisAppMentionedEventRequest {
                event_type: "praxis_app_mentioned",
                event_params,
            })
        }));
    }

    fn ingest_app_used(&mut self, input: AppUsedInput, out: &mut Vec<TrackEventRequest>) {
        let AppUsedInput { tracking, app } = input;
        let event_params = praxis_app_metadata(&tracking, app);
        out.push(TrackEventRequest::AppUsed(PraxisAppUsedEventRequest {
            event_type: "praxis_app_used",
            event_params,
        }));
    }

    fn ingest_plugin_used(&mut self, input: PluginUsedInput, out: &mut Vec<TrackEventRequest>) {
        let PluginUsedInput { tracking, plugin } = input;
        out.push(TrackEventRequest::PluginUsed(
            PraxisPluginUsedEventRequest {
                event_type: "praxis_plugin_used",
                event_params: praxis_plugin_used_metadata(&tracking, plugin),
            },
        ));
    }

    fn ingest_plugin_state_changed(
        &mut self,
        input: PluginStateChangedInput,
        out: &mut Vec<TrackEventRequest>,
    ) {
        let PluginStateChangedInput { plugin, state } = input;
        let event = PraxisPluginEventRequest {
            event_type: plugin_state_event_type(state),
            event_params: praxis_plugin_metadata(plugin),
        };
        out.push(match state {
            PluginState::Installed => TrackEventRequest::PluginInstalled(event),
            PluginState::Uninstalled => TrackEventRequest::PluginUninstalled(event),
            PluginState::Enabled => TrackEventRequest::PluginEnabled(event),
            PluginState::Disabled => TrackEventRequest::PluginDisabled(event),
        });
    }

    fn ingest_thread_initialized(
        &mut self,
        connection_id: u64,
        thread: ThreadInitializedFact,
        out: &mut Vec<TrackEventRequest>,
    ) {
        let Some(connection_state) = self.connections.get(&connection_id) else {
            return;
        };
        out.push(TrackEventRequest::ThreadInitialized(
            ThreadInitializedEvent {
                event_type: "praxis_thread_initialized",
                event_params: ThreadInitializedEventParams {
                    thread_id: thread.thread_id,
                    app_gateway_client: connection_state.app_gateway_client.clone(),
                    runtime: connection_state.runtime.clone(),
                    model: thread.model,
                    ephemeral: thread.ephemeral,
                    thread_source: thread_source_name(&thread.thread_source),
                    initialization_mode: thread.initialization_mode,
                    subagent_source: None,
                    parent_thread_id: None,
                    created_at: thread.created_at,
                },
            },
        ));
    }
}

pub(crate) fn skill_id_for_local_skill(
    repo_url: Option<&str>,
    repo_root: Option<&Path>,
    skill_path: &Path,
    skill_name: &str,
) -> String {
    let path = normalize_path_for_skill_id(repo_url, repo_root, skill_path);
    let prefix = if let Some(url) = repo_url {
        format!("repo_{url}")
    } else {
        "personal".to_string()
    };
    let raw_id = format!("{prefix}_{path}_{skill_name}");
    let mut hasher = sha1::Sha1::new();
    sha1::Digest::update(&mut hasher, raw_id.as_bytes());
    format!("{:x}", sha1::Digest::finalize(hasher))
}

/// Returns a normalized path for skill ID construction.
///
/// - Repo-scoped skills use a path relative to the repo root.
/// - User/admin/system skills use an absolute path.
pub(crate) fn normalize_path_for_skill_id(
    repo_url: Option<&str>,
    repo_root: Option<&Path>,
    skill_path: &Path,
) -> String {
    let resolved_path =
        std::fs::canonicalize(skill_path).unwrap_or_else(|_| skill_path.to_path_buf());
    match (repo_url, repo_root) {
        (Some(_), Some(root)) => {
            let resolved_root = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
            resolved_path
                .strip_prefix(&resolved_root)
                .unwrap_or(resolved_path.as_path())
                .to_string_lossy()
                .replace('\\', "/")
        }
        _ => resolved_path.to_string_lossy().replace('\\', "/"),
    }
}
