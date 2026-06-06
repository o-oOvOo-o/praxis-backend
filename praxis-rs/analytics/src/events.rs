use crate::facts::AppInvocation;
use crate::facts::InvocationType;
use crate::facts::PluginState;
use crate::facts::TrackEventsContext;
use praxis_login::default_client::originator;
use praxis_plugin::PluginTelemetryMetadata;
use praxis_protocol::protocol::SessionSource;
use serde::Serialize;

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AppGatewayRpcTransport {
    Stdio,
    Websocket,
    InProcess,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ThreadInitializationMode {
    New,
    Forked,
    Resumed,
}

#[derive(Serialize)]
pub(crate) struct TrackEventsRequest {
    pub(crate) events: Vec<TrackEventRequest>,
}

#[derive(Serialize)]
#[serde(untagged)]
pub(crate) enum TrackEventRequest {
    SkillInvocation(SkillInvocationEventRequest),
    ThreadInitialized(ThreadInitializedEvent),
    AppMentioned(PraxisAppMentionedEventRequest),
    AppUsed(PraxisAppUsedEventRequest),
    PluginUsed(PraxisPluginUsedEventRequest),
    PluginInstalled(PraxisPluginEventRequest),
    PluginUninstalled(PraxisPluginEventRequest),
    PluginEnabled(PraxisPluginEventRequest),
    PluginDisabled(PraxisPluginEventRequest),
}

#[derive(Serialize)]
pub(crate) struct SkillInvocationEventRequest {
    pub(crate) event_type: &'static str,
    pub(crate) skill_id: String,
    pub(crate) skill_name: String,
    pub(crate) event_params: SkillInvocationEventParams,
}

#[derive(Serialize)]
pub(crate) struct SkillInvocationEventParams {
    pub(crate) product_client_id: Option<String>,
    pub(crate) skill_scope: Option<String>,
    pub(crate) repo_url: Option<String>,
    pub(crate) thread_id: Option<String>,
    pub(crate) invoke_type: Option<InvocationType>,
    pub(crate) model_slug: Option<String>,
}

#[derive(Clone, Serialize)]
pub(crate) struct PraxisAppGatewayClientMetadata {
    pub(crate) product_client_id: String,
    pub(crate) client_name: Option<String>,
    pub(crate) client_version: Option<String>,
    pub(crate) rpc_transport: AppGatewayRpcTransport,
    pub(crate) experimental_api_enabled: Option<bool>,
}

#[derive(Clone, Serialize)]
pub(crate) struct PraxisRuntimeMetadata {
    pub(crate) praxis_rs_version: String,
    pub(crate) runtime_os: String,
    pub(crate) runtime_os_version: String,
    pub(crate) runtime_arch: String,
}

#[derive(Serialize)]
pub(crate) struct ThreadInitializedEventParams {
    pub(crate) thread_id: String,
    pub(crate) app_gateway_client: PraxisAppGatewayClientMetadata,
    pub(crate) runtime: PraxisRuntimeMetadata,
    pub(crate) model: String,
    pub(crate) ephemeral: bool,
    pub(crate) thread_source: Option<&'static str>,
    pub(crate) initialization_mode: ThreadInitializationMode,
    pub(crate) subagent_source: Option<String>,
    pub(crate) parent_thread_id: Option<String>,
    pub(crate) created_at: u64,
}

#[derive(Serialize)]
pub(crate) struct ThreadInitializedEvent {
    pub(crate) event_type: &'static str,
    pub(crate) event_params: ThreadInitializedEventParams,
}

#[derive(Serialize)]
pub(crate) struct PraxisAppMetadata {
    pub(crate) connector_id: Option<String>,
    pub(crate) thread_id: Option<String>,
    pub(crate) turn_id: Option<String>,
    pub(crate) app_name: Option<String>,
    pub(crate) product_client_id: Option<String>,
    pub(crate) invoke_type: Option<InvocationType>,
    pub(crate) model_slug: Option<String>,
}

#[derive(Serialize)]
pub(crate) struct PraxisAppMentionedEventRequest {
    pub(crate) event_type: &'static str,
    pub(crate) event_params: PraxisAppMetadata,
}

#[derive(Serialize)]
pub(crate) struct PraxisAppUsedEventRequest {
    pub(crate) event_type: &'static str,
    pub(crate) event_params: PraxisAppMetadata,
}

#[derive(Serialize)]
pub(crate) struct PraxisPluginMetadata {
    pub(crate) plugin_id: Option<String>,
    pub(crate) plugin_name: Option<String>,
    pub(crate) marketplace_name: Option<String>,
    pub(crate) has_skills: Option<bool>,
    pub(crate) has_llm: Option<bool>,
    pub(crate) mcp_server_count: Option<usize>,
    pub(crate) connector_ids: Option<Vec<String>>,
    pub(crate) product_client_id: Option<String>,
}

#[derive(Serialize)]
pub(crate) struct PraxisPluginUsedMetadata {
    #[serde(flatten)]
    pub(crate) plugin: PraxisPluginMetadata,
    pub(crate) thread_id: Option<String>,
    pub(crate) turn_id: Option<String>,
    pub(crate) model_slug: Option<String>,
}

#[derive(Serialize)]
pub(crate) struct PraxisPluginEventRequest {
    pub(crate) event_type: &'static str,
    pub(crate) event_params: PraxisPluginMetadata,
}

#[derive(Serialize)]
pub(crate) struct PraxisPluginUsedEventRequest {
    pub(crate) event_type: &'static str,
    pub(crate) event_params: PraxisPluginUsedMetadata,
}

pub(crate) fn plugin_state_event_type(state: PluginState) -> &'static str {
    match state {
        PluginState::Installed => "praxis_plugin_installed",
        PluginState::Uninstalled => "praxis_plugin_uninstalled",
        PluginState::Enabled => "praxis_plugin_enabled",
        PluginState::Disabled => "praxis_plugin_disabled",
    }
}

pub(crate) fn praxis_app_metadata(
    tracking: &TrackEventsContext,
    app: AppInvocation,
) -> PraxisAppMetadata {
    PraxisAppMetadata {
        connector_id: app.connector_id,
        thread_id: Some(tracking.thread_id.clone()),
        turn_id: Some(tracking.turn_id.clone()),
        app_name: app.app_name,
        product_client_id: Some(originator().value),
        invoke_type: app.invocation_type,
        model_slug: Some(tracking.model_slug.clone()),
    }
}

pub(crate) fn praxis_plugin_metadata(plugin: PluginTelemetryMetadata) -> PraxisPluginMetadata {
    let capability_summary = plugin.capability_summary;
    PraxisPluginMetadata {
        plugin_id: Some(plugin.plugin_id.as_key()),
        plugin_name: Some(plugin.plugin_id.plugin_name),
        marketplace_name: Some(plugin.plugin_id.marketplace_name),
        has_skills: capability_summary
            .as_ref()
            .map(|summary| summary.has_skills),
        has_llm: capability_summary.as_ref().map(|summary| summary.has_llm),
        mcp_server_count: capability_summary
            .as_ref()
            .map(|summary| summary.mcp_server_names.len()),
        connector_ids: capability_summary.map(|summary| {
            summary
                .app_connector_ids
                .into_iter()
                .map(|connector_id| connector_id.0)
                .collect()
        }),
        product_client_id: Some(originator().value),
    }
}

pub(crate) fn praxis_plugin_used_metadata(
    tracking: &TrackEventsContext,
    plugin: PluginTelemetryMetadata,
) -> PraxisPluginUsedMetadata {
    PraxisPluginUsedMetadata {
        plugin: praxis_plugin_metadata(plugin),
        thread_id: Some(tracking.thread_id.clone()),
        turn_id: Some(tracking.turn_id.clone()),
        model_slug: Some(tracking.model_slug.clone()),
    }
}

pub(crate) fn thread_source_name(thread_source: &SessionSource) -> Option<&'static str> {
    match thread_source {
        SessionSource::Cli
        | SessionSource::VSCode
        | SessionSource::Exec
        | SessionSource::AppGateway => Some("user"),
        SessionSource::SubAgent(_) => Some("subagent"),
        SessionSource::Mcp | SessionSource::Custom(_) | SessionSource::Unknown => None,
    }
}

pub(crate) fn current_runtime_metadata() -> PraxisRuntimeMetadata {
    let os_info = os_info::get();
    PraxisRuntimeMetadata {
        praxis_rs_version: env!("CARGO_PKG_VERSION").to_string(),
        runtime_os: std::env::consts::OS.to_string(),
        runtime_os_version: os_info.version().to_string(),
        runtime_arch: std::env::consts::ARCH.to_string(),
    }
}
