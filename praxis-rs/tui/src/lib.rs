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
use praxis_app_gateway_protocol::ThreadLookupParams;
use praxis_app_gateway_protocol::ThreadLookupSelector;
use praxis_app_gateway_protocol::ThreadSourceKind;
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
use tracing::error;
use tracing::warn;

pub(crate) const TUI_APP_GATEWAY_CHANNEL_CAPACITY: usize = 8192;
pub const DEFAULT_CENTER_CONTROL_LISTEN_URL: &str = "ws://127.0.0.1:4222";
use tracing_appender::non_blocking;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::prelude::*;
use tui_config::TuiRuntimeConfig;
use url::Url;

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
mod gateway_startup;
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
mod ratatui_runner;
mod render;
mod resume_picker;
mod run_main_entry;
mod selection_list;
mod session_log;
mod session_lookup_bootstrap;
mod shimmer;
mod skills_helpers;
mod slash_command;
mod startup_auth;
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
mod transcript;
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

use self::ratatui_runner::run_ratatui_app;

#[cfg(test)]
pub mod test_backend;
#[cfg(test)]
pub(crate) mod test_support;

pub(crate) use self::gateway_startup::AppGatewayTarget;
use self::gateway_startup::shutdown_app_gateway_if_present;
use self::gateway_startup::start_app_gateway;
pub(crate) use self::gateway_startup::start_app_gateway_for_picker;
use self::gateway_startup::start_embedded_app_gateway;
#[cfg(test)]
pub(crate) use self::gateway_startup::start_embedded_app_gateway_for_picker;
use self::gateway_startup::validate_remote_auth_token_transport;
use self::session_lookup_bootstrap::SessionLookupContext;
pub(crate) use self::session_lookup_bootstrap::build_session_lookup_config;
use self::session_lookup_bootstrap::lookup_latest_session_target_with_app_gateway;
use self::session_lookup_bootstrap::lookup_session_target_with_app_gateway;
pub(crate) use self::session_lookup_bootstrap::picker_source_switch_enabled;
pub(crate) use self::session_lookup_bootstrap::session_lookup_app_gateway_target;
use self::session_lookup_bootstrap::session_lookup_command_hint;
use self::session_lookup_bootstrap::session_lookup_params;
use self::session_lookup_bootstrap::start_session_lookup_context;
use self::startup_auth::LoadedTuiConfig;
pub(crate) use self::startup_auth::ResolveCwdOutcome;
use self::startup_auth::TerminalRestoreGuard;
pub(crate) use self::startup_auth::cwds_differ;
use self::startup_auth::determine_alt_screen_mode;
use self::startup_auth::get_login_status;
use self::startup_auth::has_any_usable_non_openai_provider;
use self::startup_auth::install_color_eyre;
use self::startup_auth::load_config_or_exit;
use self::startup_auth::load_config_or_exit_with_fallback_cwd;
use self::startup_auth::normalize_runtime_provider_model_selection;
pub(crate) use self::startup_auth::resolve_cwd_for_resume_or_fork;
use self::startup_auth::should_show_login_screen;
use self::startup_auth::should_show_trust_screen;
use crate::onboarding::onboarding_screen::OnboardingScreenArgs;
use crate::onboarding::onboarding_screen::run_onboarding_app;
use crate::provider_setup::DEEPSEEK_PROVIDER_ID;
use crate::provider_setup::DEFAULT_DEEPSEEK_MODEL;
use crate::tui::Tui;
pub use cli::Cli;
pub use cli::SessionLookupSource;
pub use gateway_startup::ControlListenConfig;
pub use gateway_startup::normalize_remote_addr;
pub use gateway_startup::parse_control_listen_addr;
pub use markdown_render::render_markdown_text;
use praxis_arg0::Arg0DispatchPaths;
pub use public_widgets::composer_input::ComposerAction;
pub use public_widgets::composer_input::ComposerInput;
pub use run_main_entry::run_main;
pub use startup_auth::LoginStatus;
// (tests access modules directly within the crate)

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
    use praxis_protocol::protocol::AskForApproval;
    use praxis_protocol::protocol::SessionSource;
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
            cwd: None,
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
        let params = session_lookup_params(
            ThreadLookupSelector::Latest,
            interactive_thread_source_kinds(/*include_non_interactive*/ false),
            None,
        );

        assert_eq!(
            params.source_kinds,
            Some(vec![ThreadSourceKind::Cli, ThreadSourceKind::VsCode])
        );
        assert_eq!(params.cwd_scope, None);
        assert_eq!(params.archived, Some(false));
        Ok(())
    }

    #[tokio::test]
    async fn latest_session_lookup_params_can_resume_non_interactive_when_requested()
    -> std::io::Result<()> {
        let params = session_lookup_params(
            ThreadLookupSelector::Latest,
            interactive_thread_source_kinds(/*include_non_interactive*/ true),
            Some("project".to_string()),
        );

        assert_eq!(params.source_kinds, None);
        assert_eq!(params.cwd_scope, Some("project".to_string()));
        assert_eq!(params.archived, Some(false));
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
            lookup_session_target_with_app_gateway(&mut app_gateway, "saved-session").await?;
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
}
