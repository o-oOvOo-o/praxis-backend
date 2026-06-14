// Forbid accidental stdout/stderr writes in the *library* portion of the TUI.
// The standalone `praxis-tui` binary prints a short help message before the
// alternate‑screen mode starts; that file opts‑out locally via `allow`.
#![deny(clippy::print_stdout, clippy::print_stderr)]
#![deny(clippy::disallowed_methods)]
use additional_dirs::add_dir_warning_message;
use app::App;
pub use app::AppExitInfo;
pub use app::ExitReason;
use app_gateway_session::AppGatewaySession;
use color_eyre::eyre::WrapErr;
use cwd_prompt::CwdPromptAction;
use cwd_prompt::CwdPromptOutcome;
use cwd_prompt::CwdSelection;
use praxis_app_gateway_client::AppGatewayClient;
use praxis_app_gateway_client::NativeAppGatewayClient;
use praxis_app_gateway_client::NativeAppGatewayClientStartArgs;
use praxis_app_gateway_client::NativeControlAuthSettings;
use praxis_app_gateway_client::RemoteAppGatewayClient;
use praxis_app_gateway_client::RemoteAppGatewayConnectArgs;
use praxis_app_gateway_protocol::AuthMode as AppGatewayAuthMode;
use praxis_app_gateway_protocol::ConfigWarningNotification;
use praxis_app_gateway_protocol::Thread as AppGatewayThread;
use praxis_app_gateway_protocol::ThreadListParams;
use praxis_app_gateway_protocol::ThreadSortKey as AppGatewayThreadSortKey;
use praxis_cloud_requirements::cloud_config_bundle_loader_for_storage;
use praxis_core::LMSTUDIO_OSS_PROVIDER_ID;
use praxis_core::ModelProviderInfo;
use praxis_core::OLLAMA_OSS_PROVIDER_ID;
use praxis_core::check_execpolicy_for_warnings;
use praxis_core::config::Config;
use praxis_core::config::ConfigBuilder;
use praxis_core::config::ConfigOverrides;
use praxis_core::config::PraxisHomeNamespace;
use praxis_core::config::current_praxis_home_namespace;
use praxis_core::config::edit::ConfigEditsBuilder;
use praxis_core::config::find_praxis_home;
use praxis_core::config::load_config_as_toml_with_cli_overrides;
use praxis_core::config::resolve_oss_provider;
use praxis_core::config_loader::CloudConfigBundleLoader;
use praxis_core::config_loader::ConfigLoadError;
use praxis_core::config_loader::LoaderOverrides;
use praxis_core::config_loader::format_config_error_with_source;
use praxis_core::format_exec_policy_error_with_source;
use praxis_core::path_utils;
use praxis_core::read_session_meta_line;
use praxis_core::windows_sandbox::WindowsSandboxLevelExt;
use praxis_login::AuthConfig;
use praxis_login::default_client::set_default_client_residency_requirement;
use praxis_login::enforce_login_restrictions;
use praxis_protocol::ThreadId;
use praxis_protocol::config_types::AltScreenMode;
use praxis_protocol::config_types::SandboxMode;
use praxis_protocol::config_types::WindowsSandboxLevel;
use praxis_protocol::openai_models::ReasoningEffort;
use praxis_protocol::protocol::AskForApproval;
use praxis_protocol::protocol::RolloutItem;
use praxis_protocol::protocol::RolloutLine;
use praxis_protocol::protocol::TurnContextItem;
use praxis_rollout::state_db::get_state_db;
use praxis_state::log_db;
use praxis_terminal_detection::Multiplexer;
use praxis_terminal_detection::terminal_info;
use praxis_utils_absolute_path::AbsolutePathBuf;
use praxis_utils_oss::ensure_oss_provider_ready;
use praxis_utils_oss::get_default_model_for_oss_provider;
use std::fs::OpenOptions;
use std::future::Future;
use std::net::SocketAddr;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use thread_pagination::interactive_thread_source_kinds;
use thread_pagination::thread_list_params as app_gateway_thread_list_params;
use tracing::error;
use tracing::warn;

pub(crate) const TUI_APP_GATEWAY_CHANNEL_CAPACITY: usize = 8192;
pub const DEFAULT_CENTER_CONTROL_LISTEN_URL: &str = "ws://127.0.0.1:4222";
use tracing_appender::non_blocking;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::prelude::*;
use tui_config::TuiRuntimeConfig;
use url::Url;
use uuid::Uuid;

mod additional_dirs;
mod app;
mod app_backtrack;
mod app_command;
mod app_event;
mod app_event_sender;
mod app_gateway_core_conversions;
mod app_gateway_session;
mod ascii_animation;
#[cfg(not(target_os = "linux"))]
mod audio_device;
#[cfg(target_os = "linux")]
#[allow(dead_code)]
mod audio_device {
    use crate::app_event::RealtimeAudioDeviceKind;

    pub(crate) fn list_realtime_audio_device_names(
        kind: RealtimeAudioDeviceKind,
    ) -> Result<Vec<String>, String> {
        Err(format!(
            "Failed to load realtime {} devices: voice input is unavailable in this build",
            kind.noun()
        ))
    }
}
mod bottom_pane;
mod chatwidget;
mod cli;
mod clipboard_paste;
mod clipboard_text;
mod collaboration_modes;
mod color;
pub mod custom_terminal;
mod cwd_prompt;
mod debug_config;
mod diff_render;
mod exec_cell;
mod exec_command;
mod external_editor;
mod file_search;
mod frames;
mod get_git_diff;
mod history_cell;
mod history_presentation;
pub mod insert_history;
mod key_hint;
mod line_truncation;
pub mod live_wrap;
mod local_chatgpt_auth;
mod markdown;
mod markdown_render;
mod markdown_stream;
mod mention_codec;
mod model_catalog;
mod model_discovery;
mod model_migration;
mod multi_agents;
mod notifications;
pub mod onboarding;
mod oss_selection;
mod pager_overlay;
mod provider_setup;
pub mod public_widgets;
mod render;
mod resume_picker;
mod selection_list;
mod session_log;
mod shimmer;
mod skills_helpers;
mod slash_command;
mod status;
mod status_indicator_widget;
mod status_runtime;
mod streaming;
mod style;
mod surface;
mod surface_theme_picker;
mod terminal_palette;
mod terminal_title;
mod text_formatting;
mod theme_picker;
mod thinking_persona;
mod thread_pagination;
mod thread_replay_policy;
mod toast_queue;
mod token_usage_summary;
mod transcript_search;
mod tui;
mod tui2;
mod tui_config;
mod turn_runtime;
mod ui_consts;
mod ui_language;
pub mod update_action;
mod update_prompt;
mod updates;
mod version;
#[cfg(not(target_os = "linux"))]
mod voice;
mod workspace;
#[cfg(target_os = "linux")]
#[allow(dead_code)]
mod voice {
    use crate::app_event_sender::AppEventSender;
    use praxis_core::config::Config;
    use praxis_protocol::protocol::RealtimeAudioFrame;
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;
    use std::sync::atomic::AtomicU16;

    pub struct VoiceCapture;

    pub(crate) struct RecordingMeterState;

    pub(crate) struct RealtimeAudioPlayer;

    impl VoiceCapture {
        pub fn start_realtime(_config: &Config, _tx: AppEventSender) -> Result<Self, String> {
            Err("voice input is unavailable in this build".to_string())
        }

        pub fn stop(self) {}

        pub fn stopped_flag(&self) -> Arc<AtomicBool> {
            Arc::new(AtomicBool::new(true))
        }

        pub fn last_peak_arc(&self) -> Arc<AtomicU16> {
            Arc::new(AtomicU16::new(0))
        }
    }

    impl RecordingMeterState {
        pub(crate) fn new() -> Self {
            Self
        }

        pub(crate) fn next_text(&mut self, _peak: u16) -> String {
            "⠤⠤⠤⠤".to_string()
        }
    }

    impl RealtimeAudioPlayer {
        pub(crate) fn start(_config: &Config) -> Result<Self, String> {
            Err("voice output is unavailable in this build".to_string())
        }

        pub(crate) fn enqueue_frame(&self, _frame: &RealtimeAudioFrame) -> Result<(), String> {
            Err("voice output is unavailable in this build".to_string())
        }

        pub(crate) fn clear(&self) {}
    }
}

mod wrapping;

#[cfg(test)]
pub mod test_backend;
#[cfg(test)]
pub(crate) mod test_support;

use crate::onboarding::onboarding_screen::OnboardingScreenArgs;
use crate::onboarding::onboarding_screen::run_onboarding_app;
use crate::provider_setup::DEEPSEEK_PROVIDER_ID;
use crate::provider_setup::DEFAULT_DEEPSEEK_MODEL;
use crate::tui::Tui;
pub use cli::Cli;
pub use cli::SessionLookupSource;
pub use markdown_render::render_markdown_text;
use praxis_arg0::Arg0DispatchPaths;
pub use public_widgets::composer_input::ComposerAction;
pub use public_widgets::composer_input::ComposerInput;
// (tests access modules directly within the crate)

async fn start_embedded_app_gateway(
    arg0_paths: Arg0DispatchPaths,
    config: Config,
    cli_kv_overrides: Vec<(String, toml::Value)>,
    loader_overrides: LoaderOverrides,
    cloud_requirements: CloudConfigBundleLoader,
    feedback: praxis_feedback::PraxisFeedback,
    control_listen: Option<SocketAddr>,
) -> color_eyre::Result<NativeAppGatewayClient> {
    start_embedded_app_gateway_with(
        arg0_paths,
        config,
        cli_kv_overrides,
        loader_overrides,
        cloud_requirements,
        feedback,
        control_listen,
        NativeAppGatewayClient::start,
    )
    .await
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum AppGatewayTarget {
    Embedded,
    Remote {
        websocket_url: String,
        auth_token: Option<String>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ControlListenConfig {
    pub addr: SocketAddr,
    pub required: bool,
}

impl ControlListenConfig {
    pub fn required(addr: SocketAddr) -> Self {
        Self {
            addr,
            required: true,
        }
    }

    pub fn best_effort(addr: SocketAddr) -> Self {
        Self {
            addr,
            required: false,
        }
    }
}

fn remote_addr_has_explicit_port(addr: &str, parsed: &Url) -> bool {
    let Some(host) = parsed.host_str() else {
        return false;
    };
    if parsed.port().is_some() {
        return true;
    }

    let Some((_, rest)) = addr.split_once("://") else {
        return false;
    };
    let authority_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let authority = &rest[..authority_end];
    let host_and_port = authority
        .rsplit_once('@')
        .map_or(authority, |(_, host_and_port)| host_and_port);
    let explicit_default_port = match parsed.scheme() {
        "ws" => 80,
        "wss" => 443,
        _ => return false,
    };
    let expected_host = if host.contains(':') {
        format!("[{host}]")
    } else {
        host.to_string()
    };
    host_and_port == format!("{expected_host}:{explicit_default_port}")
}

fn websocket_url_supports_auth_token(parsed: &Url) -> bool {
    match (parsed.scheme(), parsed.host()) {
        ("wss", Some(_)) => true,
        ("ws", Some(url::Host::Domain(domain))) => domain.eq_ignore_ascii_case("localhost"),
        ("ws", Some(url::Host::Ipv4(addr))) => addr.is_loopback(),
        ("ws", Some(url::Host::Ipv6(addr))) => addr.is_loopback(),
        _ => false,
    }
}

pub fn normalize_remote_addr(addr: &str) -> color_eyre::Result<String> {
    let parsed = match Url::parse(addr) {
        Ok(parsed) => parsed,
        Err(_) => {
            color_eyre::eyre::bail!(
                "invalid remote address `{addr}`; expected `ws://host:port` or `wss://host:port`"
            );
        }
    };
    if matches!(parsed.scheme(), "ws" | "wss")
        && parsed.host_str().is_some()
        && remote_addr_has_explicit_port(addr, &parsed)
        && parsed.path() == "/"
        && parsed.query().is_none()
        && parsed.fragment().is_none()
    {
        return Ok(parsed.to_string());
    }

    color_eyre::eyre::bail!(
        "invalid remote address `{addr}`; expected `ws://host:port` or `wss://host:port`"
    );
}

pub fn parse_control_listen_addr(addr: &str) -> color_eyre::Result<SocketAddr> {
    let Some(socket_addr) = addr.strip_prefix("ws://") else {
        color_eyre::eyre::bail!(
            "invalid control listen address `{addr}`; expected `ws://IP:PORT`"
        );
    };
    socket_addr.parse::<SocketAddr>().map_err(|err| {
        color_eyre::eyre::eyre!(
            "invalid control listen address `{addr}`; expected `ws://IP:PORT`: {err}"
        )
    })
}

fn validate_remote_auth_token_transport(websocket_url: &str) -> color_eyre::Result<()> {
    let parsed = Url::parse(websocket_url).map_err(color_eyre::Report::new)?;
    if websocket_url_supports_auth_token(&parsed) {
        return Ok(());
    }

    color_eyre::eyre::bail!(
        "remote auth tokens require `wss://` or loopback `ws://` URLs; got `{websocket_url}`"
    )
}

async fn connect_remote_app_gateway(
    websocket_url: String,
    auth_token: Option<String>,
) -> color_eyre::Result<AppGatewayClient> {
    let app_gateway = RemoteAppGatewayClient::connect(RemoteAppGatewayConnectArgs {
        websocket_url,
        auth_token,
        client_name: "praxis-tui".to_string(),
        client_version: env!("CARGO_PKG_VERSION").to_string(),
        experimental_api: true,
        opt_out_notification_methods: Vec::new(),
        channel_capacity: TUI_APP_GATEWAY_CHANNEL_CAPACITY,
    })
    .await
    .wrap_err("failed to connect to remote app gateway")?;
    Ok(AppGatewayClient::Remote(app_gateway))
}

fn control_listener_bind_failed(err: &color_eyre::Report) -> bool {
    err.chain().any(|cause| {
        cause
            .downcast_ref::<std::io::Error>()
            .is_some_and(|err| {
                matches!(
                    err.kind(),
                    std::io::ErrorKind::AddrInUse
                        | std::io::ErrorKind::AddrNotAvailable
                        | std::io::ErrorKind::PermissionDenied
                )
            })
    })
}

async fn start_app_gateway(
    target: &AppGatewayTarget,
    arg0_paths: Arg0DispatchPaths,
    config: Config,
    cli_kv_overrides: Vec<(String, toml::Value)>,
    loader_overrides: LoaderOverrides,
    cloud_requirements: CloudConfigBundleLoader,
    feedback: praxis_feedback::PraxisFeedback,
    control_listen: Option<ControlListenConfig>,
) -> color_eyre::Result<AppGatewayClient> {
    match target {
        AppGatewayTarget::Embedded => {
            let control_addr = control_listen.map(|control| control.addr);
            match start_embedded_app_gateway(
                arg0_paths.clone(),
                config.clone(),
                cli_kv_overrides.clone(),
                loader_overrides.clone(),
                cloud_requirements.clone(),
                feedback.clone(),
                control_addr,
            )
            .await
            {
                Ok(client) => Ok(AppGatewayClient::Native(client)),
                Err(err)
                    if control_listen.is_some_and(|control| !control.required)
                        && control_listener_bind_failed(&err) =>
                {
                    if let Some(control) = control_listen {
                        warn!(
                            %err,
                            control_addr = %control.addr,
                            "default Praxis Center control listener failed; continuing without external control listener"
                        );
                    }
                    start_embedded_app_gateway(
                        arg0_paths,
                        config,
                        cli_kv_overrides,
                        loader_overrides,
                        cloud_requirements,
                        feedback,
                        None,
                    )
                    .await
                    .map(AppGatewayClient::Native)
                }
                Err(err) => Err(err),
            }
        }
        AppGatewayTarget::Remote {
            websocket_url,
            auth_token,
        } => connect_remote_app_gateway(websocket_url.clone(), auth_token.clone()).await,
    }
}

pub(crate) async fn start_app_gateway_for_picker(
    config: &Config,
    target: &AppGatewayTarget,
) -> color_eyre::Result<AppGatewaySession> {
    let app_gateway = start_app_gateway(
        target,
        Arg0DispatchPaths::default(),
        config.clone(),
        Vec::new(),
        LoaderOverrides::default(),
        CloudConfigBundleLoader::default(),
        praxis_feedback::PraxisFeedback::new(),
        None,
    )
    .await?;
    Ok(AppGatewaySession::new(app_gateway))
}

#[cfg(test)]
pub(crate) async fn start_embedded_app_gateway_for_picker(
    config: &Config,
) -> color_eyre::Result<AppGatewaySession> {
    start_app_gateway_for_picker(config, &AppGatewayTarget::Embedded).await
}

async fn start_embedded_app_gateway_with<F, Fut>(
    arg0_paths: Arg0DispatchPaths,
    config: Config,
    cli_kv_overrides: Vec<(String, toml::Value)>,
    loader_overrides: LoaderOverrides,
    cloud_requirements: CloudConfigBundleLoader,
    feedback: praxis_feedback::PraxisFeedback,
    control_listen: Option<SocketAddr>,
    start_client: F,
) -> color_eyre::Result<NativeAppGatewayClient>
where
    F: FnOnce(NativeAppGatewayClientStartArgs) -> Fut,
    Fut: Future<Output = std::io::Result<NativeAppGatewayClient>>,
{
    let config_warnings = config
        .startup_warnings
        .iter()
        .map(|warning| ConfigWarningNotification {
            summary: warning.clone(),
            details: None,
            path: None,
            range: None,
        })
        .collect();
    let client = start_client(NativeAppGatewayClientStartArgs {
        arg0_paths,
        config: Arc::new(config),
        cli_overrides: cli_kv_overrides,
        loader_overrides,
        cloud_requirements,
        feedback,
        config_warnings,
        session_source: praxis_protocol::protocol::SessionSource::Cli,
        enable_praxis_api_key_env: false,
        client_name: "praxis-tui".to_string(),
        client_version: env!("CARGO_PKG_VERSION").to_string(),
        experimental_api: true,
        opt_out_notification_methods: Vec::new(),
        channel_capacity: TUI_APP_GATEWAY_CHANNEL_CAPACITY,
        control_listen,
        control_auth: NativeControlAuthSettings::default(),
    })
    .await
    .wrap_err("failed to start embedded app gateway")?;
    Ok(client)
}

async fn shutdown_app_gateway_if_present(app_gateway: Option<AppGatewaySession>) {
    if let Some(app_gateway) = app_gateway
        && let Err(err) = app_gateway.shutdown().await
    {
        warn!(%err, "Failed to shut down temporary embedded app gateway");
    }
}

fn session_target_from_app_gateway_thread(
    thread: AppGatewayThread,
) -> Option<resume_picker::SessionTarget> {
    match ThreadId::from_string(&thread.id) {
        Ok(thread_id) => Some(resume_picker::SessionTarget {
            path: thread.path,
            thread_id,
            thread_name: thread.name,
        }),
        Err(err) => {
            warn!(
                thread_id = thread.id,
                %err,
                "Ignoring app-gateway thread with invalid thread id during TUI session lookup"
            );
            None
        }
    }
}

async fn lookup_session_target_by_name_with_app_gateway(
    app_gateway: &mut AppGatewaySession,
    name: &str,
) -> color_eyre::Result<Option<resume_picker::SessionTarget>> {
    let mut cursor = None;
    loop {
        let response = app_gateway
            .thread_list(app_gateway_thread_list_params(
                cursor.clone(),
                AppGatewayThreadSortKey::UpdatedAt,
                interactive_thread_source_kinds(/*include_non_interactive*/ false),
                // Thread names are hydrated after `thread/list` resolves rollout metadata, so
                // name-based resume must scan the filtered list client-side instead of relying on
                // the backend search index.
                None,
            ))
            .await?;
        if let Some(thread) = response
            .data
            .into_iter()
            .find(|thread| thread.name.as_deref() == Some(name))
        {
            return Ok(session_target_from_app_gateway_thread(thread));
        }
        if response.next_cursor.is_none() {
            return Ok(None);
        }
        cursor = response.next_cursor;
    }
}

async fn lookup_session_target_with_app_gateway(
    app_gateway: &mut AppGatewaySession,
    id_or_name: &str,
) -> color_eyre::Result<Option<resume_picker::SessionTarget>> {
    if Uuid::parse_str(id_or_name).is_ok() {
        let thread_id = match ThreadId::from_string(id_or_name) {
            Ok(thread_id) => thread_id,
            Err(err) => {
                warn!(
                    session = id_or_name,
                    %err,
                    "Failed to parse session id during TUI lookup"
                );
                return Ok(None);
            }
        };
        return match app_gateway
            .thread_read(thread_id, /*include_turns*/ false)
            .await
        {
            Ok(thread) => Ok(session_target_from_app_gateway_thread(thread)),
            Err(err) => {
                warn!(
                    session = id_or_name,
                    %err,
                    "thread/read failed during TUI session lookup"
                );
                Ok(None)
            }
        };
    }

    lookup_session_target_by_name_with_app_gateway(app_gateway, id_or_name).await
}

async fn lookup_latest_session_target_with_app_gateway(
    app_gateway: &mut AppGatewaySession,
    cwd_filter: Option<&Path>,
    include_non_interactive: bool,
) -> color_eyre::Result<Option<resume_picker::SessionTarget>> {
    let project_filter = cwd_filter.map(project_scope_root);
    let mut cursor = None;
    loop {
        let response = app_gateway
            .thread_list(latest_session_lookup_params(
                cursor.clone(),
                include_non_interactive,
            ))
            .await?;
        if let Some(thread) =
            response
                .data
                .into_iter()
                .find(|thread| match project_filter.as_deref() {
                    Some(filter_root) => project_scope_root(thread.cwd.as_path()) == filter_root,
                    None => true,
                })
        {
            return Ok(session_target_from_app_gateway_thread(thread));
        }
        if response.next_cursor.is_none() {
            return Ok(None);
        }
        cursor = response.next_cursor;
    }
}

fn latest_session_lookup_params(
    cursor: Option<String>,
    include_non_interactive: bool,
) -> ThreadListParams {
    app_gateway_thread_list_params(
        cursor,
        AppGatewayThreadSortKey::UpdatedAt,
        interactive_thread_source_kinds(include_non_interactive),
        None,
    )
}

fn project_scope_root(path: &Path) -> PathBuf {
    praxis_git_utils::resolve_root_git_project_for_trust(path).unwrap_or_else(|| path.to_path_buf())
}

fn session_lookup_command_hint(action: &str, source: SessionLookupSource) -> String {
    match source.command_keyword() {
        Some(keyword) => format!("praxis {action} {keyword}"),
        None => format!("praxis {action}"),
    }
}

struct SessionLookupContext {
    source: SessionLookupSource,
    config: Config,
    app_gateway: AppGatewaySession,
}

pub(crate) async fn build_praxis_bridge_lookup_config(
    primary_config: &Config,
) -> std::io::Result<Config> {
    let codex_home = praxis_core::config::default_upstream_codex_home()?;
    let mut bridge_config = primary_config.clone();

    // The Codex source is a read-through lookup/fork bridge.  It may read
    // ~/.codex/sessions and auth, but Codex runtime-only config keys must not
    // block Praxis from listing threads. All Praxis-generated indexes, logs,
    // and bridge state stay under the primary Praxis home.
    let source = praxis_core::external_agent_migration::ExternalAgentSource::Codex;
    let bridge_state_home = primary_config
        .praxis_home
        .join(source.bridge_state_dir_name());
    bridge_config.praxis_home = codex_home;
    bridge_config.sqlite_home = bridge_state_home.clone();
    bridge_config.log_dir = primary_config.log_dir.join(source.bridge_log_dir_name());
    Ok(bridge_config)
}

pub(crate) async fn build_cursor_bridge_lookup_config(
    primary_config: &Config,
) -> std::io::Result<Config> {
    let mut bridge_config = primary_config.clone();
    let source = praxis_core::external_agent_migration::ExternalAgentSource::Cursor;
    let bridge_state_home = primary_config
        .praxis_home
        .join(source.bridge_state_dir_name());
    bridge_config.praxis_home = bridge_state_home.clone();
    bridge_config.sqlite_home = bridge_state_home;
    bridge_config.log_dir = primary_config.log_dir.join(source.bridge_log_dir_name());
    praxis_core::external_agent_migration::sync_external_agent_sessions_to_praxis_home(
        source,
        &bridge_config,
    )
    .await?;
    Ok(bridge_config)
}

pub(crate) async fn build_session_lookup_config(
    source: SessionLookupSource,
    primary_config: &Config,
) -> std::io::Result<Config> {
    match source {
        SessionLookupSource::Praxis => Ok(primary_config.clone()),
        SessionLookupSource::Codex => build_praxis_bridge_lookup_config(primary_config).await,
        SessionLookupSource::Cursor => build_cursor_bridge_lookup_config(primary_config).await,
    }
}

pub(crate) fn picker_source_switch_enabled(app_gateway_target: &AppGatewayTarget) -> bool {
    matches!(current_praxis_home_namespace(), PraxisHomeNamespace::Praxis)
        && matches!(app_gateway_target, AppGatewayTarget::Embedded)
}

pub(crate) fn session_lookup_app_gateway_target(
    source: SessionLookupSource,
    app_gateway_target: &AppGatewayTarget,
) -> AppGatewayTarget {
    if source.is_external() {
        AppGatewayTarget::Embedded
    } else {
        app_gateway_target.clone()
    }
}

async fn start_session_lookup_context(
    source: SessionLookupSource,
    primary_config: &Config,
    app_gateway_target: &AppGatewayTarget,
    arg0_paths: Arg0DispatchPaths,
    loader_overrides: LoaderOverrides,
    feedback: praxis_feedback::PraxisFeedback,
) -> color_eyre::Result<SessionLookupContext> {
    let lookup_config = build_session_lookup_config(source, primary_config)
        .await
        .map_err(color_eyre::Report::new)?;
    let app_gateway = start_app_gateway(
        app_gateway_target,
        arg0_paths,
        lookup_config.clone(),
        Vec::new(),
        loader_overrides,
        CloudConfigBundleLoader::default(),
        feedback,
        None,
    )
    .await?;
    Ok(SessionLookupContext {
        source,
        config: lookup_config,
        app_gateway: AppGatewaySession::new(app_gateway),
    })
}

pub async fn run_main(
    mut cli: Cli,
    arg0_paths: Arg0DispatchPaths,
    loader_overrides: LoaderOverrides,
    remote: Option<String>,
    remote_auth_token: Option<String>,
    control_listen: Option<ControlListenConfig>,
) -> std::io::Result<AppExitInfo> {
    let remote_url = remote;
    if let (Some(websocket_url), Some(_)) = (remote_url.as_deref(), remote_auth_token.as_ref()) {
        validate_remote_auth_token_transport(websocket_url).map_err(std::io::Error::other)?;
    }
    let app_gateway_target = if let Some(websocket_url) = remote_url.clone() {
        AppGatewayTarget::Remote {
            websocket_url,
            auth_token: remote_auth_token.clone(),
        }
    } else {
        AppGatewayTarget::Embedded
    };
    let (sandbox_mode, approval_policy) = if cli.full_auto {
        (
            Some(SandboxMode::WorkspaceWrite),
            Some(AskForApproval::OnRequest),
        )
    } else if cli.dangerously_bypass_approvals_and_sandbox {
        (
            Some(SandboxMode::DangerFullAccess),
            Some(AskForApproval::Never),
        )
    } else {
        (
            cli.sandbox_mode.map(Into::<SandboxMode>::into),
            cli.approval_policy.map(Into::into),
        )
    };

    // Map the legacy --search flag to the canonical web_search mode.
    if cli.web_search {
        cli.config_overrides
            .raw_overrides
            .push("web_search=\"live\"".to_string());
    }

    // When using `--oss`, let the bootstrapper pick the model (defaulting to
    // gpt-oss:20b) and ensure it is present locally. Also, force the built‑in
    let raw_overrides = cli.config_overrides.raw_overrides.clone();
    // `oss` model provider.
    let overrides_cli = praxis_utils_cli::CliConfigOverrides { raw_overrides };
    let cli_kv_overrides = match overrides_cli.parse_overrides() {
        // Parse `-c` overrides from the CLI.
        Ok(v) => v,
        #[allow(clippy::print_stderr)]
        Err(e) => {
            eprintln!("Error parsing -c overrides: {e}");
            std::process::exit(1);
        }
    };

    // we load config.toml here to determine project state.
    #[allow(clippy::print_stderr)]
    let praxis_home = match find_praxis_home() {
        Ok(praxis_home) => praxis_home.to_path_buf(),
        Err(err) => {
            eprintln!("Error finding codex home: {err}");
            std::process::exit(1);
        }
    };

    let cwd = cli.cwd.clone();
    let config_cwd = match cwd.as_deref() {
        Some(path) => AbsolutePathBuf::from_absolute_path(path.canonicalize()?)?,
        None => AbsolutePathBuf::current_dir()?,
    };

    #[allow(clippy::print_stderr)]
    let config_toml = match load_config_as_toml_with_cli_overrides(
        &praxis_home,
        &config_cwd,
        cli_kv_overrides.clone(),
    )
    .await
    {
        Ok(config_toml) => config_toml,
        Err(err) => {
            let config_error = err
                .get_ref()
                .and_then(|err| err.downcast_ref::<ConfigLoadError>())
                .map(ConfigLoadError::config_error);
            if let Some(config_error) = config_error {
                eprintln!(
                    "Error loading config.toml:\n{}",
                    format_config_error_with_source(config_error)
                );
            } else {
                eprintln!("Error loading config.toml: {err}");
            }
            std::process::exit(1);
        }
    };

    if let Err(err) =
        praxis_core::personality_migration::maybe_migrate_personality(&praxis_home, &config_toml)
            .await
    {
        tracing::warn!(error = %err, "failed to run personality migration");
    }

    let chatgpt_base_url = config_toml
        .chatgpt_base_url
        .clone()
        .unwrap_or_else(|| "https://chatgpt.com/backend-api/".to_string());
    let cloud_requirements = cloud_config_bundle_loader_for_storage(
        praxis_home.to_path_buf(),
        /*enable_praxis_api_key_env*/ false,
        config_toml.cli_auth_credentials_store.unwrap_or_default(),
        chatgpt_base_url,
    );

    let model_provider_override = if cli.oss {
        let resolved = resolve_oss_provider(
            cli.oss_provider.as_deref(),
            &config_toml,
            cli.config_profile.clone(),
        );

        if let Some(provider) = resolved {
            Some(provider)
        } else {
            // No provider configured, prompt the user
            let provider = oss_selection::select_oss_provider(&praxis_home).await?;
            if provider == "__CANCELLED__" {
                return Err(std::io::Error::other(
                    "OSS provider selection was cancelled by user",
                ));
            }
            Some(provider)
        }
    } else {
        None
    };

    // When using `--oss`, let the bootstrapper pick the model based on selected provider
    let model = if let Some(model) = &cli.model {
        Some(model.clone())
    } else if cli.oss {
        // Use the provider from model_provider_override
        model_provider_override
            .as_ref()
            .and_then(|provider_id| get_default_model_for_oss_provider(provider_id))
            .map(std::borrow::ToOwned::to_owned)
    } else {
        None // No model specified, will use the default.
    };

    let additional_dirs = cli.add_dir.clone();

    let overrides = ConfigOverrides {
        model,
        approval_policy,
        sandbox_mode,
        cwd,
        model_provider: model_provider_override.clone(),
        config_profile: cli.config_profile.clone(),
        praxis_self_exe: arg0_paths.praxis_self_exe.clone(),
        praxis_linux_sandbox_exe: arg0_paths.praxis_linux_sandbox_exe.clone(),
        main_execve_wrapper_exe: arg0_paths.main_execve_wrapper_exe.clone(),
        show_raw_agent_reasoning: cli.oss.then_some(true),
        additional_writable_roots: additional_dirs,
        ..Default::default()
    };

    let loaded_config = load_config_or_exit(
        cli_kv_overrides.clone(),
        overrides.clone(),
        cloud_requirements.clone(),
    )
    .await;
    let LoadedTuiConfig { config, tui_config } = loaded_config;

    #[allow(clippy::print_stderr)]
    match check_execpolicy_for_warnings(&config.config_layer_stack).await {
        Ok(None) => {}
        Ok(Some(err)) | Err(err) => {
            eprintln!(
                "Error loading rules:\n{}",
                format_exec_policy_error_with_source(&err)
            );
            std::process::exit(1);
        }
    }

    set_default_client_residency_requirement(config.enforce_residency.value());

    if let Some(warning) =
        add_dir_warning_message(&cli.add_dir, config.permissions.sandbox_policy.get())
    {
        #[allow(clippy::print_stderr)]
        {
            eprintln!("Error adding directories: {warning}");
            std::process::exit(1);
        }
    }

    if matches!(app_gateway_target, AppGatewayTarget::Embedded) {
        #[allow(clippy::print_stderr)]
        if let Err(err) = enforce_login_restrictions(&AuthConfig {
            praxis_home: config.praxis_home.clone(),
            auth_credentials_store_mode: config.cli_auth_credentials_store_mode,
            forced_login_method: config.forced_login_method,
            forced_chatgpt_workspace_id: config.forced_chatgpt_workspace_id.clone(),
        }) {
            eprintln!("{err}");
            std::process::exit(1);
        }
    }

    let log_dir = praxis_core::config::log_dir(&config)?;
    std::fs::create_dir_all(&log_dir)?;
    // Open (or create) your log file, appending to it.
    let mut log_file_opts = OpenOptions::new();
    log_file_opts.create(true).append(true);

    // Ensure the file is only readable and writable by the current user.
    // Doing the equivalent to `chmod 600` on Windows is quite a bit more code
    // and requires the Windows API crates, so we can reconsider that when
    // Praxis CLI is officially supported on Windows.
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        log_file_opts.mode(0o600);
    }

    let log_file = log_file_opts.open(log_dir.join("praxis-tui.log"))?;

    // Wrap file in non‑blocking writer.
    let (non_blocking, _guard) = non_blocking(log_file);

    // use RUST_LOG env var, default to info for codex crates.
    let env_filter = || {
        EnvFilter::try_from_default_env().unwrap_or_else(|_| {
            EnvFilter::new("praxis_core=info,praxis_tui=info,praxis_rmcp_client=info")
        })
    };

    let file_layer = tracing_subscriber::fmt::layer()
        .with_writer(non_blocking)
        // `with_target(true)` is the default, but we previously disabled it for file output.
        // Keep it enabled so we can selectively enable targets via `RUST_LOG=...` and then
        // grep for a specific module/target while troubleshooting.
        .with_target(true)
        .with_ansi(false)
        .with_span_events(
            tracing_subscriber::fmt::format::FmtSpan::NEW
                | tracing_subscriber::fmt::format::FmtSpan::CLOSE,
        )
        .with_filter(env_filter());

    let feedback = praxis_feedback::PraxisFeedback::new();
    let feedback_layer = feedback.logger_layer();
    let feedback_metadata_layer = feedback.metadata_layer();

    if cli.oss && model_provider_override.is_some() {
        // We're in the oss section, so provider_id should be Some
        // Let's handle None case gracefully though just in case
        let provider_id = match model_provider_override.as_ref() {
            Some(id) => id,
            None => {
                error!("OSS provider unexpectedly not set when oss flag is used");
                return Err(std::io::Error::other(
                    "OSS provider not set but oss flag was used",
                ));
            }
        };
        ensure_oss_provider_ready(provider_id, &config).await?;
    }

    let otel = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        praxis_core::otel_init::build_provider(
            &config,
            env!("CARGO_PKG_VERSION"),
            /*service_name_override*/ None,
            /*default_analytics_enabled*/ true,
        )
    })) {
        Ok(Ok(otel)) => otel,
        Ok(Err(e)) => {
            #[allow(clippy::print_stderr)]
            {
                eprintln!("Could not create otel exporter: {e}");
            }
            None
        }
        Err(_) => {
            #[allow(clippy::print_stderr)]
            {
                eprintln!("Could not create otel exporter: panicked during initialization");
            }
            None
        }
    };

    let otel_logger_layer = otel.as_ref().and_then(|o| o.logger_layer());

    let otel_tracing_layer = otel.as_ref().and_then(|o| o.tracing_layer());

    let log_db_layer = get_state_db(&config)
        .await
        .map(|db| log_db::start(db).with_filter(env_filter()));

    let _ = tracing_subscriber::registry()
        .with(file_layer)
        .with(feedback_layer)
        .with(feedback_metadata_layer)
        .with(log_db_layer)
        .with(otel_logger_layer)
        .with(otel_tracing_layer)
        .try_init();

    run_ratatui_app(
        cli,
        arg0_paths,
        loader_overrides,
        app_gateway_target,
        config,
        tui_config,
        overrides,
        cli_kv_overrides,
        cloud_requirements,
        feedback,
        remote_url,
        remote_auth_token,
        control_listen,
    )
    .await
    .map_err(|err| std::io::Error::other(err.to_string()))
}

#[allow(clippy::too_many_arguments)]
async fn run_ratatui_app(
    cli: Cli,
    arg0_paths: Arg0DispatchPaths,
    loader_overrides: LoaderOverrides,
    app_gateway_target: AppGatewayTarget,
    initial_config: Config,
    initial_tui_config: TuiRuntimeConfig,
    overrides: ConfigOverrides,
    cli_kv_overrides: Vec<(String, toml::Value)>,
    mut cloud_requirements: CloudConfigBundleLoader,
    feedback: praxis_feedback::PraxisFeedback,
    remote_url: Option<String>,
    remote_auth_token: Option<String>,
    control_listen: Option<ControlListenConfig>,
) -> color_eyre::Result<AppExitInfo> {
    let remote_mode = matches!(&app_gateway_target, AppGatewayTarget::Remote { .. });
    install_color_eyre()?;

    // Forward panic reports through tracing so they appear in the UI status
    // line, but do not swallow the default/color-eyre panic handler.
    // Chain to the previous hook so users still get a rich panic report
    // (including backtraces) after we restore the terminal.
    let prev_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        tracing::error!("panic: {info}");
        prev_hook(info);
    }));
    let mut terminal = tui::init()?;
    terminal.clear()?;

    let mut tui = Tui::new(terminal);
    let mut terminal_restore_guard = TerminalRestoreGuard::new();

    #[cfg(not(debug_assertions))]
    {
        use crate::update_prompt::UpdatePromptOutcome;

        let skip_update_prompt = cli.prompt.as_ref().is_some_and(|prompt| !prompt.is_empty());
        if !skip_update_prompt {
            match update_prompt::run_update_prompt_if_needed(&mut tui, &initial_config).await? {
                UpdatePromptOutcome::Continue => {}
                UpdatePromptOutcome::RunUpdate(action) => {
                    terminal_restore_guard.restore()?;
                    return Ok(AppExitInfo {
                        token_usage: praxis_protocol::protocol::TokenUsage::default(),
                        thread_id: None,
                        thread_name: None,
                        update_action: Some(action),
                        exit_reason: ExitReason::UserRequested,
                    });
                }
            }
        }
    }

    // Initialize high-fidelity session event logging if enabled.
    session_log::maybe_init(&initial_config);

    let should_show_trust_screen_flag = !remote_mode && should_show_trust_screen(&initial_config);
    let mut trust_decision_was_made = false;
    let has_usable_non_openai_provider = has_any_usable_non_openai_provider(&initial_config);
    let needs_openai_login_status =
        initial_config.model_provider.requires_openai_auth || !has_usable_non_openai_provider;
    let needs_onboarding_app_gateway = should_show_trust_screen_flag || needs_openai_login_status;
    let mut onboarding_app_gateway = if needs_onboarding_app_gateway {
        Some(AppGatewaySession::new(
            start_app_gateway(
                &app_gateway_target,
                arg0_paths.clone(),
                initial_config.clone(),
                cli_kv_overrides.clone(),
                loader_overrides.clone(),
                cloud_requirements.clone(),
                feedback.clone(),
                None,
            )
            .await?,
        ))
    } else {
        None
    };
    let login_status = if needs_openai_login_status {
        let Some(app_gateway) = onboarding_app_gateway.as_mut() else {
            unreachable!("onboarding app gateway should exist when auth is required");
        };
        get_login_status(app_gateway, &initial_config).await?
    } else {
        LoginStatus::NotAuthenticated
    };
    let show_login_screen = should_show_login_screen(login_status, &initial_config);
    let should_show_onboarding = should_show_trust_screen_flag || show_login_screen;

    let (mut config, mut tui_config) = if should_show_onboarding {
        let onboarding_result = run_onboarding_app(
            OnboardingScreenArgs {
                show_login_screen,
                show_trust_screen: should_show_trust_screen_flag,
                login_status,
                app_gateway_request_handle: onboarding_app_gateway
                    .as_ref()
                    .map(AppGatewaySession::request_handle),
                config: initial_config.clone(),
                tui_config: initial_tui_config.clone(),
            },
            if show_login_screen {
                onboarding_app_gateway.take()
            } else {
                None
            },
            &mut tui,
        )
        .await?;
        if onboarding_result.should_exit {
            terminal_restore_guard.restore_silently();
            session_log::log_session_end();
            let _ = tui.terminal.clear();
            return Ok(AppExitInfo {
                token_usage: praxis_protocol::protocol::TokenUsage::default(),
                thread_id: None,
                thread_name: None,
                update_action: None,
                exit_reason: ExitReason::UserRequested,
            });
        }
        trust_decision_was_made = onboarding_result.directory_trust_decision.is_some();
        // If this onboarding run included the login step, always refresh cloud requirements and
        // rebuild config. This avoids missing newly available cloud requirements due to login
        // status detection edge cases.
        if show_login_screen && !remote_mode {
            cloud_requirements = cloud_config_bundle_loader_for_storage(
                initial_config.praxis_home.clone(),
                /*enable_praxis_api_key_env*/ false,
                initial_config.cli_auth_credentials_store_mode,
                initial_config.chatgpt_base_url.clone(),
            );
        }

        // If the user made an explicit trust decision, or we showed the login flow, reload config
        // so current process state reflects persisted trust/auth changes.
        if onboarding_result.directory_trust_decision.is_some()
            || (show_login_screen && !remote_mode)
        {
            let loaded = load_config_or_exit(
                cli_kv_overrides.clone(),
                overrides.clone(),
                cloud_requirements.clone(),
            )
            .await;
            (loaded.config, loaded.tui_config)
        } else {
            (initial_config, initial_tui_config)
        }
    } else {
        shutdown_app_gateway_if_present(onboarding_app_gateway.take()).await;
        (initial_config, initial_tui_config)
    };
    shutdown_app_gateway_if_present(onboarding_app_gateway.take()).await;
    if !show_login_screen
        && let Some(selection) =
            normalize_runtime_provider_model_selection(login_status, &mut config)
        && let Err(err) = ConfigEditsBuilder::new(&config.praxis_home)
            .with_profile(config.active_profile.as_deref())
            .set_model_provider(Some(selection.provider_id.as_str()))
            .set_model(Some(selection.model.as_str()), selection.effort)
            .apply()
            .await
    {
        warn!(
            error = %err,
            provider = %selection.provider_id,
            model = %selection.model,
            "failed to persist normalized provider/model selection"
        );
    }

    let mut missing_session_exit = |id_str: &str, action: &str, source: SessionLookupSource| {
        error!("Error finding conversation path: {id_str}");
        terminal_restore_guard.restore_silently();
        session_log::log_session_end();
        let _ = tui.terminal.clear();
        Ok(AppExitInfo {
            token_usage: praxis_protocol::protocol::TokenUsage::default(),
            thread_id: None,
            thread_name: None,
            update_action: None,
            exit_reason: ExitReason::Fatal(format!(
                "No saved session found with ID {id_str}. Run `{}` without an ID to choose from existing sessions.",
                session_lookup_command_hint(action, source),
            )),
        })
    };

    let needs_app_gateway_session_lookup = cli.resume_last
        || cli.fork_last
        || cli.resume_session_id.is_some()
        || cli.fork_session_id.is_some()
        || cli.resume_picker
        || cli.fork_picker;
    let session_lookup_source = if cli.fork_picker || cli.fork_last || cli.fork_session_id.is_some()
    {
        cli.fork_source
    } else {
        cli.resume_source
    };
    let session_lookup_target =
        session_lookup_app_gateway_target(session_lookup_source, &app_gateway_target);
    let mut session_lookup_context = if needs_app_gateway_session_lookup {
        Some(
            start_session_lookup_context(
                session_lookup_source,
                &config,
                &session_lookup_target,
                arg0_paths.clone(),
                loader_overrides.clone(),
                feedback.clone(),
            )
            .await?,
        )
    } else {
        None
    };

    let use_fork = cli.fork_picker || cli.fork_last || cli.fork_session_id.is_some();
    let session_selection = if use_fork {
        if let Some(id_str) = cli.fork_session_id.as_deref() {
            let lookup_source = session_lookup_context
                .as_ref()
                .map(|ctx| ctx.source)
                .expect("session lookup app gateway should be initialized for --fork <id>");
            let Some(lookup) = session_lookup_context.as_mut() else {
                unreachable!("session lookup app gateway should be initialized for --fork <id>");
            };
            match lookup_session_target_with_app_gateway(&mut lookup.app_gateway, id_str).await? {
                Some(target_session) => resume_picker::SessionSelection::Fork(target_session),
                None => {
                    shutdown_app_gateway_if_present(
                        session_lookup_context.take().map(|ctx| ctx.app_gateway),
                    )
                    .await;
                    return missing_session_exit(id_str, "fork", lookup_source);
                }
            }
        } else if cli.fork_last {
            let Some(lookup) = session_lookup_context.as_mut() else {
                unreachable!("session lookup app gateway should be initialized for --fork --last");
            };
            match lookup_latest_session_target_with_app_gateway(
                &mut lookup.app_gateway,
                /*cwd_filter*/ None,
                /*include_non_interactive*/ false,
            )
            .await?
            {
                Some(target_session) => resume_picker::SessionSelection::Fork(target_session),
                None => resume_picker::SessionSelection::StartFresh,
            }
        } else if cli.fork_picker {
            let Some(lookup) = session_lookup_context.take() else {
                unreachable!("session lookup app gateway should be initialized for --fork picker");
            };
            let SessionLookupContext {
                source,
                config: lookup_config,
                app_gateway,
            } = lookup;
            let alternate_source = if picker_source_switch_enabled(&session_lookup_target) {
                let alternate_lookup = start_session_lookup_context(
                    source.default_alternate(),
                    &config,
                    &session_lookup_target,
                    arg0_paths.clone(),
                    loader_overrides.clone(),
                    feedback.clone(),
                )
                .await?;
                Some(resume_picker::AlternatePickerSource {
                    source: alternate_lookup.source,
                    config: alternate_lookup.config,
                    app_gateway: alternate_lookup.app_gateway,
                })
            } else {
                None
            };
            match resume_picker::run_fork_picker_with_app_gateway(
                &mut tui,
                &lookup_config,
                cli.fork_show_all,
                source,
                app_gateway,
                alternate_source,
            )
            .await?
            {
                resume_picker::SessionSelection::Exit => {
                    terminal_restore_guard.restore_silently();
                    session_log::log_session_end();
                    return Ok(AppExitInfo {
                        token_usage: praxis_protocol::protocol::TokenUsage::default(),
                        thread_id: None,
                        thread_name: None,
                        update_action: None,
                        exit_reason: ExitReason::UserRequested,
                    });
                }
                other => other,
            }
        } else {
            resume_picker::SessionSelection::StartFresh
        }
    } else if let Some(id_str) = cli.resume_session_id.as_deref() {
        let lookup_source = session_lookup_context
            .as_ref()
            .map(|ctx| ctx.source)
            .expect("session lookup app gateway should be initialized for --resume <id>");
        let Some(lookup) = session_lookup_context.as_mut() else {
            unreachable!("session lookup app gateway should be initialized for --resume <id>");
        };
        match lookup_session_target_with_app_gateway(&mut lookup.app_gateway, id_str).await? {
            Some(target_session) if lookup.source.is_external() => {
                resume_picker::SessionSelection::Fork(target_session)
            }
            Some(target_session) => resume_picker::SessionSelection::Resume(target_session),
            None => {
                shutdown_app_gateway_if_present(
                    session_lookup_context.take().map(|ctx| ctx.app_gateway),
                )
                .await;
                return missing_session_exit(id_str, "resume", lookup_source);
            }
        }
    } else if cli.resume_last {
        let filter_cwd = if cli.resume_show_all {
            None
        } else {
            Some(config.cwd.as_path())
        };
        let Some(lookup) = session_lookup_context.as_mut() else {
            unreachable!("session lookup app gateway should be initialized for --resume --last");
        };
        match lookup_latest_session_target_with_app_gateway(
            &mut lookup.app_gateway,
            filter_cwd,
            cli.resume_include_non_interactive,
        )
        .await?
        {
            Some(target_session) if lookup.source.is_external() => {
                resume_picker::SessionSelection::Fork(target_session)
            }
            Some(target_session) => resume_picker::SessionSelection::Resume(target_session),
            None => resume_picker::SessionSelection::StartFresh,
        }
    } else if cli.resume_picker {
        let Some(lookup) = session_lookup_context.take() else {
            unreachable!("session lookup app gateway should be initialized for --resume picker");
        };
        let SessionLookupContext {
            source,
            config: lookup_config,
            app_gateway,
        } = lookup;
        let alternate_source = if picker_source_switch_enabled(&session_lookup_target) {
            let alternate_lookup = start_session_lookup_context(
                source.default_alternate(),
                &config,
                &session_lookup_target,
                arg0_paths.clone(),
                loader_overrides.clone(),
                feedback.clone(),
            )
            .await?;
            Some(resume_picker::AlternatePickerSource {
                source: alternate_lookup.source,
                config: alternate_lookup.config,
                app_gateway: alternate_lookup.app_gateway,
            })
        } else {
            None
        };
        match resume_picker::run_resume_picker_with_app_gateway(
            &mut tui,
            &lookup_config,
            cli.resume_show_all,
            cli.resume_include_non_interactive,
            source,
            app_gateway,
            alternate_source,
        )
        .await?
        {
            resume_picker::SessionSelection::Resume(target_session) if source.is_external() => {
                resume_picker::SessionSelection::Fork(target_session)
            }
            resume_picker::SessionSelection::Exit => {
                terminal_restore_guard.restore_silently();
                session_log::log_session_end();
                return Ok(AppExitInfo {
                    token_usage: praxis_protocol::protocol::TokenUsage::default(),
                    thread_id: None,
                    thread_name: None,
                    update_action: None,
                    exit_reason: ExitReason::UserRequested,
                });
            }
            other => other,
        }
    } else {
        resume_picker::SessionSelection::StartFresh
    };
    shutdown_app_gateway_if_present(session_lookup_context.take().map(|ctx| ctx.app_gateway)).await;

    let workspace_mode = cli.launch_mode().is_workspace();

    let current_cwd = config.cwd.clone();
    let allow_prompt = !remote_mode && cli.cwd.is_none();
    let action_and_target_session_if_resume_or_fork = match &session_selection {
        resume_picker::SessionSelection::Resume(target_session) => {
            Some((CwdPromptAction::Resume, target_session))
        }
        resume_picker::SessionSelection::Fork(target_session) => {
            Some((CwdPromptAction::Fork, target_session))
        }
        _ => None,
    };
    let fallback_cwd = match action_and_target_session_if_resume_or_fork {
        Some((action, target_session)) => {
            if remote_mode {
                Some(current_cwd.to_path_buf())
            } else {
                match resolve_cwd_for_resume_or_fork(
                    &mut tui,
                    &config,
                    &current_cwd,
                    target_session.thread_id,
                    target_session.path.as_deref(),
                    action,
                    allow_prompt,
                )
                .await?
                {
                    ResolveCwdOutcome::Continue(cwd) => cwd,
                    ResolveCwdOutcome::Exit => {
                        terminal_restore_guard.restore_silently();
                        session_log::log_session_end();
                        return Ok(AppExitInfo {
                            token_usage: praxis_protocol::protocol::TokenUsage::default(),
                            thread_id: None,
                            thread_name: None,
                            update_action: None,
                            exit_reason: ExitReason::UserRequested,
                        });
                    }
                }
            }
        }
        None => None,
    };

    let reload_for_resume = match &session_selection {
        resume_picker::SessionSelection::Resume(_) | resume_picker::SessionSelection::Fork(_) => {
            Some(
                load_config_or_exit_with_fallback_cwd(
                    cli_kv_overrides.clone(),
                    overrides.clone(),
                    cloud_requirements.clone(),
                    fallback_cwd,
                )
                .await,
            )
        }
        _ => None,
    };
    if let Some(loaded) = reload_for_resume {
        config = loaded.config;
        tui_config = loaded.tui_config;
    }

    // Configure syntax highlighting theme from the final config — onboarding
    // and resume/fork can both reload config with a different TUI theme, so
    // this must happen after the last possible reload.
    if let Some(w) = crate::render::highlight::set_theme_override(
        tui_config.theme.clone(),
        find_praxis_home().ok(),
    ) {
        config.startup_warnings.push(w);
    }

    set_default_client_residency_requirement(config.enforce_residency.value());
    let active_profile = config.active_profile.clone();
    let should_show_trust_screen = should_show_trust_screen(&config);
    let should_prompt_windows_sandbox_nux_at_startup = cfg!(target_os = "windows")
        && trust_decision_was_made
        && WindowsSandboxLevel::from_config(&config) == WindowsSandboxLevel::Disabled;

    let Cli {
        prompt,
        images,
        no_alt_screen,
        ..
    } = cli;

    let use_alt_screen =
        workspace_mode || determine_alt_screen_mode(no_alt_screen, tui_config.alternate_screen);
    tui.set_alt_screen_enabled(use_alt_screen);
    tui.set_mouse_capture_enabled(workspace_mode)?;
    let app_gateway = match start_app_gateway(
        &app_gateway_target,
        arg0_paths,
        config.clone(),
        cli_kv_overrides.clone(),
        loader_overrides,
        cloud_requirements.clone(),
        feedback.clone(),
        control_listen,
    )
    .await
    {
        Ok(app_gateway) => app_gateway,
        Err(err) => {
            terminal_restore_guard.restore_silently();
            session_log::log_session_end();
            return Err(err);
        }
    };

    if use_alt_screen {
        tui.enter_alt_screen()?;
    }

    let app_result = App::run(
        &mut tui,
        AppGatewaySession::new(app_gateway),
        config,
        tui_config,
        cli_kv_overrides.clone(),
        overrides.clone(),
        active_profile,
        prompt,
        images,
        session_selection,
        feedback,
        should_show_trust_screen, // Proxy to: is it a first run in this directory?
        should_prompt_windows_sandbox_nux_at_startup,
        remote_url,
        remote_auth_token,
        workspace_mode,
    )
    .await;

    while tui.is_alt_screen_active() {
        let _ = tui.leave_alt_screen();
    }
    terminal_restore_guard.restore_silently();
    // Mark the end of the recorded session.
    session_log::log_session_end();
    // ignore error when collecting usage – report underlying error instead
    app_result
}

fn install_color_eyre() -> color_eyre::Result<()> {
    match color_eyre::install() {
        Ok(()) => Ok(()),
        Err(err) if err.to_string().contains("hook has already been installed") => {
            tracing::debug!(error = %err, "color-eyre hook was already installed");
            Ok(())
        }
        Err(err) => Err(err),
    }
}

pub(crate) async fn resolve_session_thread_id(
    path: &Path,
    id_str_if_uuid: Option<&str>,
) -> Option<ThreadId> {
    match id_str_if_uuid {
        Some(id_str) => ThreadId::from_string(id_str).ok(),
        None => read_session_meta_line(path)
            .await
            .ok()
            .map(|meta_line| meta_line.meta.id),
    }
}

pub(crate) async fn read_session_cwd(
    config: &Config,
    thread_id: ThreadId,
    path: Option<&Path>,
) -> Option<PathBuf> {
    if let Some(state_db_ctx) = get_state_db(config).await
        && let Ok(Some(metadata)) = state_db_ctx.get_thread(thread_id).await
    {
        return Some(metadata.cwd);
    }

    // Prefer the latest TurnContext cwd so resume/fork reflects the most recent
    // session directory (for the changed-cwd prompt) when DB data is unavailable.
    // The alternative would be mutating the SessionMeta line when the session cwd
    // changes, but the rollout is an append-only JSONL log and rewriting the head
    // would be error-prone.
    let path = path?;
    if let Some(cwd) = read_latest_turn_context(path).await.map(|item| item.cwd) {
        return Some(cwd);
    }
    match read_session_meta_line(path).await {
        Ok(meta_line) => Some(meta_line.meta.cwd),
        Err(err) => {
            let rollout_path = path.display().to_string();
            tracing::warn!(
                %rollout_path,
                %err,
                "Failed to read session metadata from rollout"
            );
            None
        }
    }
}

pub(crate) async fn read_session_model(
    config: &Config,
    thread_id: ThreadId,
    path: Option<&Path>,
) -> Option<String> {
    if let Some(state_db_ctx) = get_state_db(config).await
        && let Ok(Some(metadata)) = state_db_ctx.get_thread(thread_id).await
        && let Some(model) = metadata.model
    {
        return Some(model);
    }

    let path = path?;
    read_latest_turn_context(path).await.map(|item| item.model)
}

async fn read_latest_turn_context(path: &Path) -> Option<TurnContextItem> {
    let text = tokio::fs::read_to_string(path).await.ok()?;
    for line in text.lines().rev() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Ok(rollout_line) = serde_json::from_str::<RolloutLine>(trimmed) else {
            continue;
        };
        if let RolloutItem::TurnContext(item) = rollout_line.item {
            return Some(item);
        }
    }
    None
}

pub(crate) fn cwds_differ(current_cwd: &Path, session_cwd: &Path) -> bool {
    match (
        path_utils::normalize_for_path_comparison(current_cwd),
        path_utils::normalize_for_path_comparison(session_cwd),
    ) {
        (Ok(current), Ok(session)) => current != session,
        _ => current_cwd != session_cwd,
    }
}

pub(crate) enum ResolveCwdOutcome {
    Continue(Option<PathBuf>),
    Exit,
}

pub(crate) async fn resolve_cwd_for_resume_or_fork(
    tui: &mut Tui,
    config: &Config,
    current_cwd: &Path,
    thread_id: ThreadId,
    path: Option<&Path>,
    action: CwdPromptAction,
    allow_prompt: bool,
) -> color_eyre::Result<ResolveCwdOutcome> {
    let Some(history_cwd) = read_session_cwd(config, thread_id, path).await else {
        return Ok(ResolveCwdOutcome::Continue(None));
    };
    if allow_prompt && cwds_differ(current_cwd, &history_cwd) {
        let selection_outcome =
            cwd_prompt::run_cwd_selection_prompt(tui, action, current_cwd, &history_cwd).await?;
        return Ok(match selection_outcome {
            CwdPromptOutcome::Selection(CwdSelection::Current) => {
                ResolveCwdOutcome::Continue(Some(current_cwd.to_path_buf()))
            }
            CwdPromptOutcome::Selection(CwdSelection::Session) => {
                ResolveCwdOutcome::Continue(Some(history_cwd))
            }
            CwdPromptOutcome::Exit => ResolveCwdOutcome::Exit,
        });
    }
    Ok(ResolveCwdOutcome::Continue(Some(history_cwd)))
}

#[expect(
    clippy::print_stderr,
    reason = "TUI should no longer be displayed, so we can write to stderr."
)]
fn restore() {
    if let Err(err) = tui::restore() {
        eprintln!(
            "failed to restore terminal. Run `reset` or restart your terminal to recover: {err}"
        );
    }
}

struct TerminalRestoreGuard {
    active: bool,
}

impl TerminalRestoreGuard {
    fn new() -> Self {
        Self { active: true }
    }

    #[cfg_attr(debug_assertions, allow(dead_code))]
    fn restore(&mut self) -> color_eyre::Result<()> {
        if self.active {
            crate::tui::restore()?;
            self.active = false;
        }
        Ok(())
    }

    fn restore_silently(&mut self) {
        if self.active {
            restore();
            self.active = false;
        }
    }
}

impl Drop for TerminalRestoreGuard {
    fn drop(&mut self) {
        self.restore_silently();
    }
}

/// Determine whether to use the terminal's alternate screen buffer.
///
/// The alternate screen buffer provides a cleaner fullscreen experience without polluting
/// the terminal's scrollback history. However, it conflicts with terminal multiplexers like
/// Zellij that strictly follow the xterm spec, which disallows scrollback in alternate screen
/// buffers. Zellij intentionally disables scrollback in alternate screen mode (see
/// https://github.com/zellij-org/zellij/pull/1032) and offers no configuration option to
/// change this behavior.
///
/// This function implements a pragmatic workaround:
/// - If `--no-alt-screen` is explicitly passed, always disable alternate screen
/// - Otherwise, respect the `tui.alternate_screen` config setting:
///   - `always`: Use alternate screen everywhere (original behavior)
///   - `never`: Inline mode only, preserves scrollback
///   - `auto` (default): Auto-detect the terminal multiplexer and disable alternate screen
///     only in Zellij, enabling it everywhere else
fn determine_alt_screen_mode(no_alt_screen: bool, tui_alternate_screen: AltScreenMode) -> bool {
    if no_alt_screen {
        false
    } else {
        match tui_alternate_screen {
            AltScreenMode::Always => true,
            AltScreenMode::Never => false,
            AltScreenMode::Auto => {
                let terminal_info = terminal_info();
                !matches!(terminal_info.multiplexer, Some(Multiplexer::Zellij { .. }))
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoginStatus {
    AuthMode(AppGatewayAuthMode),
    NotAuthenticated,
}

struct LoadedTuiConfig {
    config: Config,
    tui_config: TuiRuntimeConfig,
}

async fn get_login_status(
    app_gateway: &mut AppGatewaySession,
    config: &Config,
) -> color_eyre::Result<LoginStatus> {
    if !config
        .model_providers
        .values()
        .any(|provider| provider.requires_openai_auth)
    {
        return Ok(LoginStatus::NotAuthenticated);
    }

    let bootstrap = app_gateway.bootstrap(config).await?;
    Ok(match bootstrap.account_auth_mode {
        Some(auth_mode) => LoginStatus::AuthMode(auth_mode),
        None => LoginStatus::NotAuthenticated,
    })
}

async fn load_config_or_exit(
    cli_kv_overrides: Vec<(String, toml::Value)>,
    overrides: ConfigOverrides,
    cloud_requirements: CloudConfigBundleLoader,
) -> LoadedTuiConfig {
    load_config_or_exit_with_fallback_cwd(
        cli_kv_overrides,
        overrides,
        cloud_requirements,
        /*fallback_cwd*/ None,
    )
    .await
}

async fn load_config_or_exit_with_fallback_cwd(
    cli_kv_overrides: Vec<(String, toml::Value)>,
    overrides: ConfigOverrides,
    cloud_requirements: CloudConfigBundleLoader,
    fallback_cwd: Option<PathBuf>,
) -> LoadedTuiConfig {
    #[allow(clippy::print_stderr)]
    match ConfigBuilder::default()
        .cli_overrides(cli_kv_overrides)
        .harness_overrides(overrides)
        .cloud_config_bundle(cloud_requirements)
        .fallback_cwd(fallback_cwd)
        .build()
        .await
    {
        Ok(config) => {
            let tui_config = match TuiRuntimeConfig::from_core_config(&config) {
                Ok(tui_config) => tui_config,
                Err(err) => {
                    eprintln!("Error loading TUI configuration: {err}");
                    std::process::exit(1);
                }
            };
            LoadedTuiConfig { config, tui_config }
        }
        Err(err) => {
            eprintln!("Error loading configuration: {err}");
            std::process::exit(1);
        }
    }
}

/// Determine if the user has decided whether to trust the current directory.
fn should_show_trust_screen(config: &Config) -> bool {
    config.active_project.trust_level.is_none()
}

fn should_show_login_screen(login_status: LoginStatus, config: &Config) -> bool {
    if active_provider_is_usable(login_status, config) {
        return false;
    }

    !has_any_usable_provider(login_status, config)
}

fn active_provider_is_usable(login_status: LoginStatus, config: &Config) -> bool {
    if config.model_provider.requires_openai_auth {
        return login_status != LoginStatus::NotAuthenticated;
    }
    provider_has_configured_credentials(&config.model_provider)
        || is_local_oss_provider(&config.model_provider_id, &config.model_provider)
}

fn has_any_usable_provider(login_status: LoginStatus, config: &Config) -> bool {
    let has_openai_login = login_status != LoginStatus::NotAuthenticated;
    if has_openai_login
        && config
            .model_providers
            .values()
            .any(|provider| provider.requires_openai_auth)
    {
        return true;
    }
    has_any_usable_non_openai_provider(config)
}

fn has_any_usable_non_openai_provider(config: &Config) -> bool {
    config.model_providers.values().any(|provider| {
        !provider.requires_openai_auth && provider_has_configured_credentials(provider)
    })
}

fn first_usable_non_openai_provider(config: &Config) -> Option<(String, ModelProviderInfo)> {
    config
        .model_providers
        .iter()
        .find(|(_provider_id, provider)| {
            !provider.requires_openai_auth && provider_has_configured_credentials(provider)
        })
        .map(|(provider_id, provider)| (provider_id.clone(), provider.clone()))
}

fn provider_has_configured_credentials(provider: &ModelProviderInfo) -> bool {
    if provider.requires_openai_auth {
        return false;
    }
    if provider.auth.is_some() {
        return true;
    }
    if provider
        .experimental_bearer_token
        .as_deref()
        .is_some_and(|value| !value.trim().is_empty())
    {
        return true;
    }
    if let Some(env_key) = provider.env_key.as_deref()
        && std::env::var(env_key)
            .ok()
            .is_some_and(|value| !value.trim().is_empty())
    {
        return true;
    }
    false
}

fn is_local_oss_provider(provider_id: &str, provider: &ModelProviderInfo) -> bool {
    if provider_id == OLLAMA_OSS_PROVIDER_ID || provider_id == LMSTUDIO_OSS_PROVIDER_ID {
        return true;
    }
    provider
        .base_url
        .as_deref()
        .is_some_and(is_loopback_base_url)
}

fn is_loopback_base_url(raw: &str) -> bool {
    let Ok(url) = Url::parse(raw) else {
        return false;
    };
    matches!(
        url.host_str(),
        Some("localhost") | Some("127.0.0.1") | Some("::1")
    )
}

#[derive(Debug, Clone)]
struct PersistedRuntimeModelSelection {
    provider_id: String,
    model: String,
    effort: Option<ReasoningEffort>,
}

fn normalize_runtime_provider_model_selection(
    login_status: LoginStatus,
    config: &mut Config,
) -> Option<PersistedRuntimeModelSelection> {
    if active_provider_is_usable(login_status, config) {
        return normalize_active_provider_model(config);
    }
    let Some((provider_id, provider)) = first_usable_non_openai_provider(config) else {
        return None;
    };
    let Some(model) = startup_model_for_provider(config, provider_id.as_str(), &provider) else {
        return None;
    };
    warn!(
        active_provider = %config.model_provider_id,
        fallback_provider = %provider_id,
        fallback_model = %model,
        "active provider is unavailable; using configured fallback provider for this run"
    );
    config.model_provider_id = provider_id;
    config.model_provider = provider;
    config.model = Some(model);
    None
}

fn normalize_active_provider_model(config: &mut Config) -> Option<PersistedRuntimeModelSelection> {
    if !is_deepseek_provider(&config.model_provider_id, &config.model_provider) {
        return None;
    }
    if config
        .model
        .as_deref()
        .is_some_and(is_supported_deepseek_model)
    {
        return None;
    }

    let previous_model = config.model.clone().unwrap_or_default();
    let model = DEFAULT_DEEPSEEK_MODEL.to_string();
    warn!(
        provider = %config.model_provider_id,
        previous_model = %previous_model,
        normalized_model = %model,
        "active provider does not support the configured model; normalizing selection"
    );
    config.model = Some(model.clone());
    Some(PersistedRuntimeModelSelection {
        provider_id: config.model_provider_id.clone(),
        model,
        effort: config.model_reasoning_effort,
    })
}

fn startup_model_for_provider(
    config: &Config,
    provider_id: &str,
    provider: &ModelProviderInfo,
) -> Option<String> {
    if is_deepseek_provider(provider_id, provider) {
        return Some(DEFAULT_DEEPSEEK_MODEL.to_string());
    }
    config.model.clone()
}

fn is_deepseek_provider(provider_id: &str, provider: &ModelProviderInfo) -> bool {
    provider_id == DEEPSEEK_PROVIDER_ID
        || provider.name.eq_ignore_ascii_case("deepseek")
        || provider
            .base_url
            .as_deref()
            .is_some_and(|base_url| base_url.contains("api.deepseek.com"))
}

fn is_supported_deepseek_model(model: &str) -> bool {
    matches!(
        model.trim().to_ascii_lowercase().as_str(),
        "deepseek-v4-pro" | "deepseek-v4-flash"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use praxis_app_gateway_protocol::ClientRequest;
    use praxis_app_gateway_protocol::RequestId;
    use praxis_app_gateway_protocol::ThreadStartParams;
    use praxis_app_gateway_protocol::ThreadStartResponse;
    use praxis_core::config::ConfigBuilder;
    use praxis_core::config::ConfigOverrides;
    use praxis_core::config::ProjectConfig;
    use praxis_features::Feature;
    use praxis_protocol::protocol::AskForApproval;
    use praxis_protocol::protocol::RolloutItem;
    use praxis_protocol::protocol::RolloutLine;
    use praxis_protocol::protocol::SessionMeta;
    use praxis_protocol::protocol::SessionMetaLine;
    use praxis_protocol::protocol::SessionSource;
    use praxis_protocol::protocol::TurnContextItem;
    use serial_test::serial;
    use tempfile::TempDir;

    async fn build_config(temp_dir: &TempDir) -> std::io::Result<Config> {
        ConfigBuilder::default()
            .praxis_home(temp_dir.path().to_path_buf())
            .build()
            .await
    }

    async fn start_test_embedded_app_gateway(
        config: Config,
    ) -> color_eyre::Result<NativeAppGatewayClient> {
        start_embedded_app_gateway(
            Arg0DispatchPaths::default(),
            config,
            Vec::new(),
            LoaderOverrides::default(),
            CloudConfigBundleLoader::default(),
            praxis_feedback::PraxisFeedback::new(),
        )
        .await
    }

    #[test]
    fn session_target_display_label_falls_back_to_thread_id() {
        let thread_id = ThreadId::new();
        let target = crate::resume_picker::SessionTarget {
            path: None,
            thread_id,
            thread_name: None,
        };

        assert_eq!(target.display_label(), format!("thread {thread_id}"));
    }

    #[test]
    fn normalize_remote_addr_accepts_websocket_url() {
        assert_eq!(
            normalize_remote_addr("ws://127.0.0.1:4500").expect("ws URL should normalize"),
            "ws://127.0.0.1:4500/"
        );
    }

    #[test]
    fn normalize_remote_addr_accepts_secure_websocket_url() {
        assert_eq!(
            normalize_remote_addr("wss://example.com:443").expect("wss URL should normalize"),
            "wss://example.com/"
        );
    }

    #[test]
    fn normalize_remote_addr_rejects_websocket_url_without_explicit_port() {
        for addr in [
            "ws://127.0.0.1",
            "wss://example.com",
            "ws://user:pass@127.0.0.1",
        ] {
            let err = normalize_remote_addr(addr)
                .expect_err("websocket URLs without an explicit port should be rejected");
            assert!(
                err.to_string()
                    .contains("expected `ws://host:port` or `wss://host:port`")
            );
        }
    }

    #[test]
    fn normalize_remote_addr_rejects_invalid_input() {
        let err = normalize_remote_addr("https://127.0.0.1:4500")
            .expect_err("https URLs should be rejected");
        assert!(
            err.to_string()
                .contains("expected `ws://host:port` or `wss://host:port`")
        );
    }

    #[test]
    fn normalize_remote_addr_rejects_host_port_shortcut() {
        let err =
            normalize_remote_addr("127.0.0.1:4500").expect_err("host:port should be rejected");
        assert!(
            err.to_string()
                .contains("expected `ws://host:port` or `wss://host:port`")
        );
    }

    #[test]
    fn remote_auth_token_transport_accepts_loopback_ws() {
        validate_remote_auth_token_transport("ws://127.0.0.1:4500/")
            .expect("loopback ws should be allowed for auth tokens");
        validate_remote_auth_token_transport("ws://localhost:4500/")
            .expect("localhost ws should be allowed for auth tokens");
        validate_remote_auth_token_transport("ws://[::1]:4500/")
            .expect("ipv6 loopback ws should be allowed for auth tokens");
    }

    #[test]
    fn remote_auth_token_transport_accepts_secure_wss() {
        validate_remote_auth_token_transport("wss://example.com:443/")
            .expect("wss should be allowed for auth tokens");
    }

    #[test]
    fn remote_auth_token_transport_rejects_non_loopback_ws() {
        let err = validate_remote_auth_token_transport("ws://example.com:4500/")
            .expect_err("non-loopback ws should be rejected for auth tokens");
        assert!(
            err.to_string()
                .contains("remote auth tokens require `wss://` or loopback `ws://` URLs")
        );
    }

    #[tokio::test]
    async fn latest_session_lookup_params_are_provider_agnostic_for_embedded_sessions()
    -> std::io::Result<()> {
        let params = latest_session_lookup_params(
            /*cursor*/ None, /*include_non_interactive*/ false,
        );

        assert_eq!(params.model_providers, None);
        assert_eq!(params.cwd, None);
        Ok(())
    }

    #[tokio::test]
    async fn latest_session_lookup_params_can_resume_non_interactive_when_requested()
    -> std::io::Result<()> {
        let params = latest_session_lookup_params(
            /*cursor*/ Some("cursor-1".to_string()),
            /*include_non_interactive*/ true,
        );

        assert_eq!(params.cursor, Some("cursor-1".to_string()));
        assert_eq!(params.model_providers, None);
        assert_eq!(params.cwd, None);
        assert_eq!(params.source_kinds, None);
        Ok(())
    }

    #[tokio::test]
    async fn read_session_cwd_returns_none_without_sqlite_or_rollout_path() -> std::io::Result<()> {
        let temp_dir = TempDir::new()?;
        let config = build_config(&temp_dir).await?;

        let cwd = read_session_cwd(&config, ThreadId::new(), /*path*/ None).await;

        assert_eq!(cwd, None);
        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn windows_shows_trust_prompt_without_sandbox() -> std::io::Result<()> {
        let temp_dir = TempDir::new()?;
        let mut config = build_config(&temp_dir).await?;
        config.active_project = ProjectConfig { trust_level: None };
        config.set_windows_sandbox_enabled(/*value*/ false);

        let should_show = should_show_trust_screen(&config);
        assert!(
            should_show,
            "Trust prompt should be shown when project trust is undecided"
        );
        Ok(())
    }

    #[tokio::test]
    async fn embedded_app_gateway_supports_thread_start_rpc() -> color_eyre::Result<()> {
        let temp_dir = TempDir::new()?;
        let config = build_config(&temp_dir).await?;
        let app_gateway = start_test_embedded_app_gateway(config).await?;
        let response: ThreadStartResponse = app_gateway
            .request_typed(ClientRequest::ThreadStart {
                request_id: RequestId::Integer(1),
                params: ThreadStartParams {
                    ephemeral: Some(true),
                    ..ThreadStartParams::default()
                },
            })
            .await
            .expect("thread/start should succeed");
        assert!(!response.thread.id.is_empty());

        app_gateway.shutdown().await?;
        Ok(())
    }

    #[tokio::test]
    async fn lookup_session_target_by_name_ignores_backend_search_term_mismatch()
    -> color_eyre::Result<()> {
        let temp_dir = TempDir::new()?;
        let config = build_config(&temp_dir).await?;
        let thread_id = ThreadId::new();
        let rollout_path = temp_dir
            .path()
            .join("sessions/2025/02/01")
            .join(format!("rollout-2025-02-01T10-00-00-{thread_id}.jsonl"));
        let rollout_dir = rollout_path.parent().expect("rollout parent");
        std::fs::create_dir_all(rollout_dir)?;
        std::fs::write(&rollout_path, "")?;

        let state_runtime = praxis_state::StateRuntime::init(
            config.praxis_home.clone(),
            config.model_provider_id.clone(),
        )
        .await
        .map_err(std::io::Error::other)?;
        state_runtime
            .mark_backfill_complete(/*last_watermark*/ None)
            .await
            .map_err(std::io::Error::other)?;

        let session_cwd = temp_dir.path().join("project");
        std::fs::create_dir_all(&session_cwd)?;
        let created_at = chrono::DateTime::parse_from_rfc3339("2025-02-01T10:00:00Z")
            .expect("timestamp should parse")
            .with_timezone(&chrono::Utc);
        let mut builder = praxis_state::ThreadMetadataBuilder::new(
            thread_id,
            rollout_path.clone(),
            created_at,
            SessionSource::Cli,
        );
        builder.cwd = session_cwd;
        let mut metadata = builder.build(config.model_provider_id.as_str());
        metadata.title = "Different rollout title".to_string();
        metadata.first_user_message = Some("preview text".to_string());
        state_runtime
            .upsert_thread(&metadata)
            .await
            .map_err(std::io::Error::other)?;

        praxis_rollout::ThreadNameWriter::new(Some(state_runtime.as_ref()))
            .write_name(thread_id, "saved-session")
            .await?;

        let mut app_gateway =
            AppGatewaySession::new(praxis_app_gateway_client::AppGatewayClient::Native(
                start_test_embedded_app_gateway(config).await?,
            ));
        let target =
            lookup_session_target_by_name_with_app_gateway(&mut app_gateway, "saved-session")
                .await?;
        let target = target.expect("name lookup should find the saved thread");
        assert_eq!(target.path, Some(rollout_path));
        assert_eq!(target.thread_id, thread_id);

        app_gateway.shutdown().await?;
        Ok(())
    }

    #[tokio::test]
    async fn embedded_app_gateway_start_failure_is_returned() -> color_eyre::Result<()> {
        let temp_dir = TempDir::new()?;
        let config = build_config(&temp_dir).await?;
        let result = start_embedded_app_gateway_with(
            Arg0DispatchPaths::default(),
            config,
            Vec::new(),
            LoaderOverrides::default(),
            CloudConfigBundleLoader::default(),
            praxis_feedback::PraxisFeedback::new(),
            |_args| async { Err(std::io::Error::other("boom")) },
        )
        .await;
        let err = match result {
            Ok(_) => panic!("startup failure should be returned"),
            Err(err) => err,
        };

        assert!(
            err.to_string()
                .contains("failed to start embedded app gateway"),
            "error should preserve the embedded app gateway startup context"
        );
        Ok(())
    }
    #[tokio::test]
    #[serial]
    async fn windows_shows_trust_prompt_with_sandbox() -> std::io::Result<()> {
        let temp_dir = TempDir::new()?;
        let mut config = build_config(&temp_dir).await?;
        config.active_project = ProjectConfig { trust_level: None };
        config.set_windows_sandbox_enabled(/*value*/ true);

        let should_show = should_show_trust_screen(&config);
        if cfg!(target_os = "windows") {
            assert!(
                should_show,
                "Windows trust prompt should be shown on native Windows with sandbox enabled"
            );
        } else {
            assert!(
                should_show,
                "Non-Windows should still show trust prompt when project is untrusted"
            );
        }
        Ok(())
    }
    #[tokio::test]
    async fn untrusted_project_skips_trust_prompt() -> std::io::Result<()> {
        use praxis_protocol::config_types::TrustLevel;
        let temp_dir = TempDir::new()?;
        let mut config = build_config(&temp_dir).await?;
        config.active_project = ProjectConfig {
            trust_level: Some(TrustLevel::Untrusted),
        };

        let should_show = should_show_trust_screen(&config);
        assert!(
            !should_show,
            "Trust prompt should not be shown for projects explicitly marked as untrusted"
        );
        Ok(())
    }

    fn build_turn_context(config: &Config, cwd: PathBuf) -> TurnContextItem {
        let model = config
            .model
            .clone()
            .unwrap_or_else(|| "gpt-5.1".to_string());
        TurnContextItem {
            turn_id: None,
            trace_id: None,
            cwd,
            current_date: None,
            timezone: None,
            approval_policy: config.permissions.approval_policy.value(),
            sandbox_policy: config.permissions.sandbox_policy.get().clone(),
            network: None,
            model,
            personality: None,
            collaboration_mode: None,
            realtime_active: Some(false),
            effort: config.model_reasoning_effort,
            summary: config
                .model_reasoning_summary
                .unwrap_or(praxis_protocol::config_types::ReasoningSummary::Auto),
            user_instructions: None,
            developer_instructions: None,
            final_output_json_schema: None,
            truncation_policy: None,
        }
    }

    #[tokio::test]
    async fn read_session_cwd_prefers_latest_turn_context() -> std::io::Result<()> {
        let temp_dir = TempDir::new()?;
        let config = build_config(&temp_dir).await?;
        let first = temp_dir.path().join("first");
        let second = temp_dir.path().join("second");
        std::fs::create_dir_all(&first)?;
        std::fs::create_dir_all(&second)?;

        let rollout_path = temp_dir.path().join("rollout.jsonl");
        let lines = vec![
            RolloutLine {
                timestamp: "t0".to_string(),
                item: RolloutItem::TurnContext(build_turn_context(&config, first)),
            },
            RolloutLine {
                timestamp: "t1".to_string(),
                item: RolloutItem::TurnContext(build_turn_context(&config, second.clone())),
            },
        ];
        let mut text = String::new();
        for line in lines {
            text.push_str(&serde_json::to_string(&line).expect("serialize rollout"));
            text.push('\n');
        }
        std::fs::write(&rollout_path, text)?;

        let cwd = read_session_cwd(&config, ThreadId::new(), Some(&rollout_path))
            .await
            .expect("expected cwd");
        assert_eq!(cwd, second);
        Ok(())
    }

    #[tokio::test]
    async fn should_prompt_when_meta_matches_current_but_latest_turn_differs() -> std::io::Result<()>
    {
        let temp_dir = TempDir::new()?;
        let config = build_config(&temp_dir).await?;
        let current = temp_dir.path().join("current");
        let latest = temp_dir.path().join("latest");
        std::fs::create_dir_all(&current)?;
        std::fs::create_dir_all(&latest)?;

        let rollout_path = temp_dir.path().join("rollout.jsonl");
        let session_meta = SessionMeta {
            cwd: current.clone(),
            ..SessionMeta::default()
        };
        let lines = vec![
            RolloutLine {
                timestamp: "t0".to_string(),
                item: RolloutItem::SessionMeta(SessionMetaLine {
                    meta: session_meta,
                    git: None,
                }),
            },
            RolloutLine {
                timestamp: "t1".to_string(),
                item: RolloutItem::TurnContext(build_turn_context(&config, latest.clone())),
            },
        ];
        let mut text = String::new();
        for line in lines {
            text.push_str(&serde_json::to_string(&line).expect("serialize rollout"));
            text.push('\n');
        }
        std::fs::write(&rollout_path, text)?;

        let session_cwd = read_session_cwd(&config, ThreadId::new(), Some(&rollout_path))
            .await
            .expect("expected cwd");
        assert_eq!(session_cwd, latest);
        assert!(cwds_differ(&current, &session_cwd));
        Ok(())
    }

    #[tokio::test]
    async fn config_rebuild_changes_trust_defaults_with_cwd() -> std::io::Result<()> {
        let temp_dir = TempDir::new()?;
        let praxis_home = temp_dir.path().to_path_buf();
        let trusted = temp_dir.path().join("trusted");
        let untrusted = temp_dir.path().join("untrusted");
        std::fs::create_dir_all(&trusted)?;
        std::fs::create_dir_all(&untrusted)?;

        // TOML keys need escaped backslashes on Windows paths.
        let trusted_display = trusted.display().to_string().replace('\\', "\\\\");
        let untrusted_display = untrusted.display().to_string().replace('\\', "\\\\");
        let config_toml = format!(
            r#"[projects."{trusted_display}"]
trust_level = "trusted"

[projects."{untrusted_display}"]
trust_level = "untrusted"
"#
        );
        std::fs::write(temp_dir.path().join("config.toml"), config_toml)?;

        let trusted_overrides = ConfigOverrides {
            cwd: Some(trusted.clone()),
            ..Default::default()
        };
        let trusted_config = ConfigBuilder::default()
            .praxis_home(praxis_home.clone())
            .harness_overrides(trusted_overrides.clone())
            .build()
            .await?;
        assert_eq!(
            trusted_config.permissions.approval_policy.value(),
            AskForApproval::OnRequest
        );

        let untrusted_overrides = ConfigOverrides {
            cwd: Some(untrusted),
            ..trusted_overrides
        };
        let untrusted_config = ConfigBuilder::default()
            .praxis_home(praxis_home)
            .harness_overrides(untrusted_overrides)
            .build()
            .await?;
        assert_eq!(
            untrusted_config.permissions.approval_policy.value(),
            AskForApproval::UnlessTrusted
        );
        Ok(())
    }

    /// Regression: theme must be configured from the *final* config.
    ///
    /// `run_ratatui_app` can reload config during onboarding and again
    /// during session resume/fork.  The syntax theme override (stored in
    /// a `OnceLock`) must use the final TUI config's theme, not the
    /// initial one — otherwise users resuming a thread in a project with
    /// a different theme get the wrong highlighting.
    ///
    /// We verify the invariant indirectly: `validate_theme_name` (the
    /// pure validation core of `set_theme_override`) must be called with
    /// the *final* config's theme, and its warning must land in the
    /// final config's `startup_warnings`.
    #[tokio::test]
    async fn theme_warning_uses_final_config() -> std::io::Result<()> {
        use crate::render::highlight::validate_theme_name;

        let temp_dir = TempDir::new()?;

        // initial_config has a valid theme — no warning.
        let initial_config = build_config(&temp_dir).await?;
        let initial_tui_config = TuiRuntimeConfig::from_core_config(&initial_config)?;
        assert!(initial_tui_config.theme.is_none());

        // Simulate resume/fork reload: the final config has an invalid theme.
        let mut config = build_config(&temp_dir).await?;
        let mut tui_config = TuiRuntimeConfig::from_core_config(&config)?;
        tui_config.theme = Some("bogus-theme".into());

        // Theme override must use the final config (not initial_config).
        // This mirrors the real call site in run_ratatui_app.
        if let Some(w) = validate_theme_name(tui_config.theme.as_deref(), Some(temp_dir.path())) {
            config.startup_warnings.push(w);
        }

        assert_eq!(
            config.startup_warnings.len(),
            1,
            "warning from final config's invalid theme should be present"
        );
        assert!(
            config.startup_warnings[0].contains("bogus-theme"),
            "warning should reference the final config's theme name"
        );
        Ok(())
    }

    #[tokio::test]
    async fn read_session_cwd_falls_back_to_session_meta() -> std::io::Result<()> {
        let temp_dir = TempDir::new()?;
        let config = build_config(&temp_dir).await?;
        let session_cwd = temp_dir.path().join("session");
        std::fs::create_dir_all(&session_cwd)?;

        let rollout_path = temp_dir.path().join("rollout.jsonl");
        let session_meta = SessionMeta {
            cwd: session_cwd.clone(),
            ..SessionMeta::default()
        };
        let meta_line = RolloutLine {
            timestamp: "t0".to_string(),
            item: RolloutItem::SessionMeta(SessionMetaLine {
                meta: session_meta,
                git: None,
            }),
        };
        let text = format!(
            "{}\n",
            serde_json::to_string(&meta_line).expect("serialize meta")
        );
        std::fs::write(&rollout_path, text)?;

        let cwd = read_session_cwd(&config, ThreadId::new(), Some(&rollout_path))
            .await
            .expect("expected cwd");
        assert_eq!(cwd, session_cwd);
        Ok(())
    }

    #[tokio::test]
    async fn read_session_cwd_prefers_sqlite_when_thread_id_present() -> std::io::Result<()> {
        let temp_dir = TempDir::new()?;
        let mut config = build_config(&temp_dir).await?;
        config
            .features
            .enable(Feature::Sqlite)
            .expect("test config should allow sqlite");

        let thread_id = ThreadId::new();
        let rollout_cwd = temp_dir.path().join("rollout-cwd");
        let sqlite_cwd = temp_dir.path().join("sqlite-cwd");
        std::fs::create_dir_all(&rollout_cwd)?;
        std::fs::create_dir_all(&sqlite_cwd)?;

        let rollout_path = temp_dir.path().join("rollout.jsonl");
        let rollout_line = RolloutLine {
            timestamp: "t0".to_string(),
            item: RolloutItem::TurnContext(build_turn_context(&config, rollout_cwd)),
        };
        std::fs::write(
            &rollout_path,
            format!(
                "{}\n",
                serde_json::to_string(&rollout_line).expect("serialize rollout")
            ),
        )?;

        let runtime = praxis_state::StateRuntime::init(
            config.praxis_home.clone(),
            config.model_provider_id.clone(),
        )
        .await
        .map_err(std::io::Error::other)?;
        runtime
            .mark_backfill_complete(/*last_watermark*/ None)
            .await
            .map_err(std::io::Error::other)?;

        let mut builder = praxis_state::ThreadMetadataBuilder::new(
            thread_id,
            rollout_path.clone(),
            chrono::Utc::now(),
            SessionSource::Cli,
        );
        builder.cwd = sqlite_cwd.clone();
        let metadata = builder.build(config.model_provider_id.as_str());
        runtime
            .upsert_thread(&metadata)
            .await
            .map_err(std::io::Error::other)?;

        let cwd = read_session_cwd(&config, thread_id, Some(&rollout_path))
            .await
            .expect("expected cwd");
        assert_eq!(cwd, sqlite_cwd);
        Ok(())
    }
}
