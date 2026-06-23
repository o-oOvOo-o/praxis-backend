use super::*;

pub(super) fn install_color_eyre() -> color_eyre::Result<()> {
    match color_eyre::install() {
        Ok(()) => Ok(()),
        Err(err) if err.to_string().contains("hook has already been installed") => {
            tracing::debug!(error = %err, "color-eyre hook was already installed");
            Ok(())
        }
        Err(err) => Err(err),
    }
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
    current_cwd: &Path,
    session_cwd: Option<&Path>,
    action: CwdPromptAction,
    allow_prompt: bool,
) -> color_eyre::Result<ResolveCwdOutcome> {
    let Some(history_cwd) = session_cwd else {
        return Ok(ResolveCwdOutcome::Continue(None));
    };
    let history_cwd = history_cwd.to_path_buf();
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
pub(super) fn restore() {
    if let Err(err) = tui::restore() {
        eprintln!(
            "failed to restore terminal. Run `reset` or restart your terminal to recover: {err}"
        );
    }
}

pub(super) struct TerminalRestoreGuard {
    active: bool,
}

impl TerminalRestoreGuard {
    pub(super) fn new() -> Self {
        Self { active: true }
    }

    #[cfg_attr(debug_assertions, allow(dead_code))]
    pub(super) fn restore(&mut self) -> color_eyre::Result<()> {
        if self.active {
            crate::tui::restore()?;
            self.active = false;
        }
        Ok(())
    }

    pub(super) fn restore_silently(&mut self) {
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
pub(super) fn determine_alt_screen_mode(
    no_alt_screen: bool,
    tui_alternate_screen: AltScreenMode,
) -> bool {
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

pub(super) struct LoadedTuiConfig {
    pub(super) config: Config,
    pub(super) tui_config: TuiRuntimeConfig,
}

pub(super) async fn get_login_status(
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

pub(super) async fn load_config_or_exit(
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

pub(super) async fn load_config_or_exit_with_fallback_cwd(
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
pub(super) fn should_show_trust_screen(config: &Config) -> bool {
    config.active_project.trust_level.is_none()
}

pub(super) fn should_show_login_screen(login_status: LoginStatus, config: &Config) -> bool {
    if active_provider_is_usable(login_status, config) {
        return false;
    }

    !has_any_usable_provider(login_status, config)
}

pub(super) fn active_provider_is_usable(login_status: LoginStatus, config: &Config) -> bool {
    if config.model_provider.requires_openai_auth {
        return login_status != LoginStatus::NotAuthenticated;
    }
    provider_has_configured_credentials(&config.model_provider)
        || is_local_oss_provider(&config.model_provider_id, &config.model_provider)
}

pub(super) fn has_any_usable_provider(login_status: LoginStatus, config: &Config) -> bool {
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

pub(super) fn has_any_usable_non_openai_provider(config: &Config) -> bool {
    config.model_providers.values().any(|provider| {
        !provider.requires_openai_auth && provider_has_configured_credentials(provider)
    })
}

pub(super) fn first_usable_non_openai_provider(
    config: &Config,
) -> Option<(String, ModelProviderInfo)> {
    config
        .model_providers
        .iter()
        .find(|(_provider_id, provider)| {
            !provider.requires_openai_auth && provider_has_configured_credentials(provider)
        })
        .map(|(provider_id, provider)| (provider_id.clone(), provider.clone()))
}

pub(super) fn provider_has_configured_credentials(provider: &ModelProviderInfo) -> bool {
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

pub(super) fn is_local_oss_provider(provider_id: &str, provider: &ModelProviderInfo) -> bool {
    if provider_id == OLLAMA_OSS_PROVIDER_ID || provider_id == LMSTUDIO_OSS_PROVIDER_ID {
        return true;
    }
    provider
        .base_url
        .as_deref()
        .is_some_and(is_loopback_base_url)
}

pub(super) fn is_loopback_base_url(raw: &str) -> bool {
    let Ok(url) = Url::parse(raw) else {
        return false;
    };
    matches!(
        url.host_str(),
        Some("localhost") | Some("127.0.0.1") | Some("::1")
    )
}

#[derive(Debug, Clone)]
pub(super) struct PersistedRuntimeModelSelection {
    pub(super) provider_id: String,
    pub(super) model: String,
    pub(super) effort: Option<ReasoningEffort>,
}

pub(super) fn normalize_runtime_provider_model_selection(
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

pub(super) fn normalize_active_provider_model(
    config: &mut Config,
) -> Option<PersistedRuntimeModelSelection> {
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

pub(super) fn startup_model_for_provider(
    config: &Config,
    provider_id: &str,
    provider: &ModelProviderInfo,
) -> Option<String> {
    if is_deepseek_provider(provider_id, provider) {
        return Some(DEFAULT_DEEPSEEK_MODEL.to_string());
    }
    config.model.clone()
}

pub(super) fn is_deepseek_provider(provider_id: &str, provider: &ModelProviderInfo) -> bool {
    provider_id == DEEPSEEK_PROVIDER_ID
        || provider.name.eq_ignore_ascii_case("deepseek")
        || provider
            .base_url
            .as_deref()
            .is_some_and(|base_url| base_url.contains("api.deepseek.com"))
}

pub(super) fn is_supported_deepseek_model(model: &str) -> bool {
    matches!(
        model.trim().to_ascii_lowercase().as_str(),
        "deepseek-v4-pro" | "deepseek-v4-flash"
    )
}
