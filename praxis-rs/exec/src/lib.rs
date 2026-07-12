// - In the default output mode, it is paramount that the only thing written to
//   stdout is the final message (if any).
// - In --json mode, stdout must be valid JSONL, one event per line.
// For both modes, any other output must be written to stderr.
#![deny(clippy::print_stdout)]

mod cli;
mod event_processor;
mod event_processor_with_human_output;
pub mod event_processor_with_jsonl_output;
pub mod exec_events;

pub use cli::Cli;
pub use cli::Command;
pub use cli::ReviewArgs;
use event_processor_with_human_output::EventProcessorWithHumanOutput;
use event_processor_with_jsonl_output::EventProcessorWithJsonOutput;
use praxis_app_gateway_client::AppGatewayClient;
use praxis_app_gateway_client::AppGatewayEvent;
use praxis_app_gateway_client::DEFAULT_NATIVE_GATEWAY_CHANNEL_CAPACITY;
use praxis_app_gateway_client::NativeAppGatewayClient;
use praxis_app_gateway_client::NativeAppGatewayClientStartArgs;
use praxis_app_gateway_client::NativeControlAuthSettings;
use praxis_app_gateway_client::RemoteAppGatewayClient;
use praxis_app_gateway_client::RemoteAppGatewayConnectArgs;
use praxis_app_gateway_protocol::ClientRequest;
use praxis_app_gateway_protocol::ConfigWarningNotification;
use praxis_app_gateway_protocol::JSONRPCErrorError;
use praxis_app_gateway_protocol::McpServerElicitationAction;
use praxis_app_gateway_protocol::McpServerElicitationRequestResponse;
use praxis_app_gateway_protocol::RequestId;
use praxis_app_gateway_protocol::ReviewStartParams;
use praxis_app_gateway_protocol::ReviewStartResponse;
use praxis_app_gateway_protocol::ReviewTarget as ApiReviewTarget;
use praxis_app_gateway_protocol::ServerNotification;
use praxis_app_gateway_protocol::ServerRequest;
use praxis_app_gateway_protocol::Thread as AppGatewayThread;
use praxis_app_gateway_protocol::ThreadItem as AppGatewayThreadItem;
use praxis_app_gateway_protocol::ThreadListParams;
use praxis_app_gateway_protocol::ThreadListResponse;
use praxis_app_gateway_protocol::ThreadReadParams;
use praxis_app_gateway_protocol::ThreadReadResponse;
use praxis_app_gateway_protocol::ThreadResumeParams;
use praxis_app_gateway_protocol::ThreadResumeResponse;
use praxis_app_gateway_protocol::ThreadSortKey;
use praxis_app_gateway_protocol::ThreadSourceKind;
use praxis_app_gateway_protocol::ThreadStartParams;
use praxis_app_gateway_protocol::ThreadStartResponse;
use praxis_app_gateway_protocol::ThreadUnsubscribeParams;
use praxis_app_gateway_protocol::ThreadUnsubscribeResponse;
use praxis_app_gateway_protocol::TurnInterruptParams;
use praxis_app_gateway_protocol::TurnInterruptResponse;
use praxis_app_gateway_protocol::TurnStartParams;
use praxis_app_gateway_protocol::TurnStartResponse;
use praxis_app_gateway_protocol::TurnStartedNotification;
use praxis_arg0::Arg0DispatchPaths;
use praxis_cloud_requirements::cloud_config_bundle_loader_for_storage;
use praxis_core::LMSTUDIO_OSS_PROVIDER_ID;
use praxis_core::OLLAMA_OSS_PROVIDER_ID;
use praxis_core::check_execpolicy_for_warnings;
use praxis_core::config::Config;
use praxis_core::config::ConfigBuilder;
use praxis_core::config::ConfigOverrides;
use praxis_core::config::find_praxis_home;
use praxis_core::config::load_config_as_toml_with_cli_overrides;
use praxis_core::config::resolve_oss_provider;
use praxis_core::config_loader::ConfigLoadError;
use praxis_core::config_loader::LoaderOverrides;
use praxis_core::config_loader::format_config_error_with_source;
use praxis_core::format_exec_policy_error_with_source;
use praxis_core::path_utils;
use praxis_feedback::PraxisFeedback;
use praxis_git_utils::get_git_repo_root;
use praxis_login::AuthConfig;
use praxis_login::default_client::set_default_client_residency_requirement;
use praxis_login::default_client::set_default_originator;
use praxis_login::enforce_login_restrictions;
use praxis_otel::set_parent_from_context;
use praxis_otel::traceparent_context_from_env;
use praxis_protocol::config_types::SandboxMode;
use praxis_protocol::protocol::AskForApproval;
use praxis_protocol::protocol::ReviewRequest;
use praxis_protocol::protocol::ReviewTarget;
use praxis_protocol::protocol::RolloutItem;
use praxis_protocol::protocol::RolloutLine;
use praxis_protocol::protocol::SandboxPolicy;
use praxis_protocol::protocol::SessionConfiguredEvent;
use praxis_protocol::protocol::SessionSource;
use praxis_protocol::user_input::UserInput;
use praxis_utils_absolute_path::AbsolutePathBuf;
use praxis_utils_oss::ensure_oss_provider_ready;
use praxis_utils_oss::get_default_model_for_oss_provider;
use serde_json::Value;
use std::collections::HashMap;
use std::io::IsTerminal;
use std::io::Read;
use std::path::Path;
use std::path::PathBuf;
use supports_color::Stream;
use tokio::sync::mpsc;
use tracing::Instrument;
use tracing::error;
use tracing::field;
use tracing::info;
use tracing::info_span;
use tracing::warn;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::prelude::*;
use uuid::Uuid;

use crate::cli::Command as ExecCommand;
use crate::event_processor::EventProcessor;
use crate::event_processor::PraxisStatus;

#[path = "lib/prompt_input.rs"]
mod prompt_input;
#[path = "lib/server_requests.rs"]
mod server_requests;
#[path = "lib/session_resolution.rs"]
mod session_resolution;
#[cfg(test)]
#[path = "lib/tests.rs"]
mod tests;

use prompt_input::{
    PromptDecodeError, build_review_request, decode_prompt_bytes, load_output_schema,
    prompt_with_stdin_context, read_prompt_from_stdin, resolve_prompt, resolve_root_prompt,
};
use server_requests::{
    canceled_mcp_server_elicitation_response, handle_server_request, request_shutdown,
};
use session_resolution::{
    lagged_event_warning_message, maybe_backfill_turn_completed_items, resolve_resume_thread_id,
    should_process_notification, turn_items_for_thread,
};

const DEFAULT_ANALYTICS_ENABLED: bool = true;
enum InitialOperation {
    UserTurn {
        items: Vec<UserInput>,
        output_schema: Option<Value>,
    },
    Review {
        review_request: ReviewRequest,
    },
}

enum StdinPromptBehavior {
    /// Read stdin only when there is no positional prompt, which is the legacy
    /// `praxis exec` behavior for `praxis exec` with piped input.
    RequiredIfPiped,
    /// Always treat stdin as the prompt, used for the explicit `praxis exec -`
    /// sentinel and similar forced-stdin call sites.
    Forced,
    /// If stdin is piped alongside a positional prompt, treat stdin as
    /// additional context to append rather than as the primary prompt.
    OptionalAppend,
}

struct RequestIdSequencer {
    next: i64,
}

impl RequestIdSequencer {
    fn new() -> Self {
        Self { next: 1 }
    }

    fn next(&mut self) -> RequestId {
        let id = self.next;
        self.next += 1;
        RequestId::Integer(id)
    }
}

struct ExecRunArgs {
    in_process_start_args: NativeAppGatewayClientStartArgs,
    remote_app_gateway: Option<ExecRemoteAppGateway>,
    command: Option<ExecCommand>,
    config: Config,
    dangerously_bypass_approvals_and_sandbox: bool,
    exec_span: tracing::Span,
    images: Vec<PathBuf>,
    json_mode: bool,
    last_message_file: Option<PathBuf>,
    model_provider: Option<String>,
    oss: bool,
    output_schema_path: Option<PathBuf>,
    prompt: Option<String>,
    skip_git_repo_check: bool,
    stderr_with_ansi: bool,
}

#[derive(Clone, Debug)]
pub struct ExecRemoteAppGateway {
    pub websocket_url: String,
    pub auth_token: Option<String>,
}

async fn connect_remote_exec_app_gateway(
    remote: ExecRemoteAppGateway,
) -> anyhow::Result<AppGatewayClient> {
    let client = RemoteAppGatewayClient::connect(RemoteAppGatewayConnectArgs {
        websocket_url: remote.websocket_url,
        auth_token: remote.auth_token,
        client_name: "praxis_exec".to_string(),
        client_version: env!("CARGO_PKG_VERSION").to_string(),
        experimental_api: true,
        opt_out_notification_methods: Vec::new(),
        host_extensions: Vec::new(),
        channel_capacity: DEFAULT_NATIVE_GATEWAY_CHANNEL_CAPACITY,
    })
    .await
    .map_err(|err| anyhow::anyhow!("failed to connect remote app-gateway client: {err}"))?;
    Ok(AppGatewayClient::Remote(client))
}

async fn start_exec_app_gateway(
    in_process_start_args: NativeAppGatewayClientStartArgs,
    remote_app_gateway: Option<ExecRemoteAppGateway>,
) -> anyhow::Result<AppGatewayClient> {
    if let Some(remote) = remote_app_gateway {
        return connect_remote_exec_app_gateway(remote).await;
    }

    let client = NativeAppGatewayClient::start(in_process_start_args)
        .await
        .map_err(|err| {
            anyhow::anyhow!("failed to initialize embedded app-gateway client: {err}")
        })?;
    Ok(AppGatewayClient::Native(client))
}

fn exec_root_span() -> tracing::Span {
    info_span!(
        "praxis.exec",
        otel.kind = "internal",
        thread.id = field::Empty,
        turn.id = field::Empty,
    )
}

pub async fn run_main(
    cli: Cli,
    arg0_paths: Arg0DispatchPaths,
    remote_app_gateway: Option<ExecRemoteAppGateway>,
) -> anyhow::Result<()> {
    if let Err(err) = set_default_originator("praxis_exec".to_string()) {
        tracing::warn!(
            ?err,
            "Failed to set praxis exec originator override {err:?}"
        );
    }

    let Cli {
        command,
        images,
        model: model_cli_arg,
        oss,
        oss_provider,
        config_profile,
        full_auto,
        dangerously_bypass_approvals_and_sandbox,
        cwd,
        skip_git_repo_check,
        add_dir,
        ephemeral,
        color,
        last_message_file,
        json: json_mode,
        sandbox_mode: sandbox_mode_cli_arg,
        prompt,
        output_schema: output_schema_path,
        config_overrides,
    } = cli;

    let (_stdout_with_ansi, stderr_with_ansi) = match color {
        cli::Color::Always => (true, true),
        cli::Color::Never => (false, false),
        cli::Color::Auto => (
            supports_color::on_cached(Stream::Stdout).is_some(),
            supports_color::on_cached(Stream::Stderr).is_some(),
        ),
    };
    // Build fmt layer (existing logging) to compose with OTEL layer.
    let default_level = "error";

    // Build env_filter separately and attach via with_filter.
    let env_filter = EnvFilter::try_from_default_env()
        .or_else(|_| EnvFilter::try_new(default_level))
        .unwrap_or_else(|_| EnvFilter::new(default_level));

    let fmt_layer = tracing_subscriber::fmt::layer()
        .with_ansi(stderr_with_ansi)
        .with_writer(std::io::stderr)
        .with_filter(env_filter);

    let sandbox_mode = if full_auto {
        Some(SandboxMode::WorkspaceWrite)
    } else if dangerously_bypass_approvals_and_sandbox {
        Some(SandboxMode::DangerFullAccess)
    } else {
        sandbox_mode_cli_arg.map(Into::<SandboxMode>::into)
    };

    // Parse `-c` overrides from the CLI.
    let cli_kv_overrides = match config_overrides.parse_overrides() {
        Ok(v) => v,
        #[allow(clippy::print_stderr)]
        Err(e) => {
            eprintln!("Error parsing -c overrides: {e}");
            std::process::exit(1);
        }
    };

    let resolved_cwd = cwd.clone();
    let config_cwd = match resolved_cwd.as_deref() {
        Some(path) => AbsolutePathBuf::from_absolute_path(path.canonicalize()?)?,
        None => AbsolutePathBuf::current_dir()?,
    };

    // we load config.toml here to determine project state.
    #[allow(clippy::print_stderr)]
    let praxis_home = match find_praxis_home() {
        Ok(praxis_home) => praxis_home,
        Err(err) => {
            eprintln!("Error finding praxis home: {err}");
            std::process::exit(1);
        }
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

    let chatgpt_base_url = config_toml
        .chatgpt_base_url
        .clone()
        .unwrap_or_else(|| "https://chatgpt.com/backend-api/".to_string());
    // TODO(gt): Make cloud requirements failures blocking once we can fail-closed.
    let cloud_requirements = cloud_config_bundle_loader_for_storage(
        praxis_home.clone(),
        /*enable_praxis_api_key_env*/ false,
        config_toml.cli_auth_credentials_store.unwrap_or_default(),
        chatgpt_base_url,
    );
    let run_cli_overrides = cli_kv_overrides.clone();
    let run_loader_overrides = LoaderOverrides::default();
    let run_cloud_requirements = cloud_requirements.clone();

    let model_provider = if oss {
        let resolved = resolve_oss_provider(
            oss_provider.as_deref(),
            &config_toml,
            config_profile.clone(),
        );

        if let Some(provider) = resolved {
            Some(provider)
        } else {
            return Err(anyhow::anyhow!(
                "No default OSS provider configured. Use --local-provider=provider or set oss_provider to one of: {LMSTUDIO_OSS_PROVIDER_ID}, {OLLAMA_OSS_PROVIDER_ID} in config.toml"
            ));
        }
    } else {
        None // No OSS mode enabled
    };

    // When using `--oss`, let the bootstrapper pick the model based on selected provider
    let model = if let Some(model) = model_cli_arg {
        Some(model)
    } else if oss {
        model_provider
            .as_ref()
            .and_then(|provider_id| get_default_model_for_oss_provider(provider_id))
            .map(std::borrow::ToOwned::to_owned)
    } else {
        None // No model specified, will use the default.
    };

    // Load configuration and determine approval policy
    let overrides = ConfigOverrides {
        model,
        review_model: None,
        config_profile,
        // Default to never ask for approvals in headless mode. Feature flags can override.
        approval_policy: Some(AskForApproval::Never),
        approvals_reviewer: None,
        sandbox_mode,
        cwd: resolved_cwd,
        model_provider: model_provider.clone(),
        service_tier: None,
        praxis_self_exe: arg0_paths.praxis_self_exe.clone(),
        praxis_linux_sandbox_exe: arg0_paths.praxis_linux_sandbox_exe.clone(),
        main_execve_wrapper_exe: arg0_paths.main_execve_wrapper_exe.clone(),
        zsh_path: None,
        base_instructions: None,
        developer_instructions: None,
        personality: None,
        compact_prompt: None,
        include_apply_patch_tool: None,
        show_raw_agent_reasoning: oss.then_some(true),
        tools_web_search_request: None,
        ephemeral: ephemeral.then_some(true),
        additional_writable_roots: add_dir,
    };

    let config = ConfigBuilder::default()
        .cli_overrides(cli_kv_overrides)
        .harness_overrides(overrides)
        .cloud_config_bundle(cloud_requirements)
        .build()
        .await?;

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

    if let Err(err) = enforce_login_restrictions(&AuthConfig {
        praxis_home: config.praxis_home.clone(),
        auth_credentials_store_mode: config.cli_auth_credentials_store_mode,
        forced_login_method: config.forced_login_method,
        forced_chatgpt_workspace_id: config.forced_chatgpt_workspace_id.clone(),
    }) {
        eprintln!("{err}");
        std::process::exit(1);
    }

    let otel = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        praxis_core::otel_init::build_provider(
            &config,
            env!("CARGO_PKG_VERSION"),
            /*service_name_override*/ None,
            DEFAULT_ANALYTICS_ENABLED,
        )
    })) {
        Ok(Ok(otel)) => otel,
        Ok(Err(e)) => {
            eprintln!("Could not create otel exporter: {e}");
            None
        }
        Err(_) => {
            eprintln!("Could not create otel exporter: panicked during initialization");
            None
        }
    };

    let otel_logger_layer = otel.as_ref().and_then(|o| o.logger_layer());

    let otel_tracing_layer = otel.as_ref().and_then(|o| o.tracing_layer());

    let _ = tracing_subscriber::registry()
        .with(fmt_layer)
        .with(otel_tracing_layer)
        .with(otel_logger_layer)
        .try_init();

    let exec_span = exec_root_span();
    if let Some(context) = traceparent_context_from_env() {
        set_parent_from_context(&exec_span, context);
    }
    let config_warnings: Vec<ConfigWarningNotification> = config
        .startup_warnings
        .iter()
        .map(|warning| ConfigWarningNotification {
            summary: warning.clone(),
            details: None,
            path: None,
            range: None,
        })
        .collect();
    let in_process_start_args = NativeAppGatewayClientStartArgs {
        arg0_paths,
        config: std::sync::Arc::new(config.clone()),
        cli_overrides: run_cli_overrides,
        loader_overrides: run_loader_overrides,
        cloud_requirements: run_cloud_requirements.into(),
        feedback: PraxisFeedback::new(),
        config_warnings,
        session_source: SessionSource::Exec,
        enable_praxis_api_key_env: true,
        client_name: "praxis_exec".to_string(),
        client_version: env!("CARGO_PKG_VERSION").to_string(),
        experimental_api: true,
        opt_out_notification_methods: Vec::new(),
        host_extensions: Vec::new(),
        channel_capacity: DEFAULT_NATIVE_GATEWAY_CHANNEL_CAPACITY,
        control_listen: None,
        control_auth: NativeControlAuthSettings::default(),
    };
    run_exec_session(ExecRunArgs {
        in_process_start_args,
        remote_app_gateway,
        command,
        config,
        dangerously_bypass_approvals_and_sandbox,
        exec_span: exec_span.clone(),
        images,
        json_mode,
        last_message_file,
        model_provider,
        oss,
        output_schema_path,
        prompt,
        skip_git_repo_check,
        stderr_with_ansi,
    })
    .instrument(exec_span)
    .await
}

async fn run_exec_session(args: ExecRunArgs) -> anyhow::Result<()> {
    let ExecRunArgs {
        in_process_start_args,
        remote_app_gateway,
        command,
        config,
        dangerously_bypass_approvals_and_sandbox,
        exec_span,
        images,
        json_mode,
        last_message_file,
        model_provider,
        oss,
        output_schema_path,
        prompt,
        skip_git_repo_check,
        stderr_with_ansi,
    } = args;

    let mut event_processor: Box<dyn EventProcessor> = match json_mode {
        true => Box::new(EventProcessorWithJsonOutput::new(last_message_file.clone())),
        _ => Box::new(EventProcessorWithHumanOutput::create_with_ansi(
            stderr_with_ansi,
            &config,
            last_message_file.clone(),
        )),
    };
    if oss {
        // We're in the oss section, so provider_id should be Some
        // Let's handle None case gracefully though just in case
        let provider_id = match model_provider.as_ref() {
            Some(id) => id,
            None => {
                error!("OSS provider unexpectedly not set when oss flag is used");
                return Err(anyhow::anyhow!(
                    "OSS provider not set but oss flag was used"
                ));
            }
        };
        ensure_oss_provider_ready(provider_id, &config)
            .await
            .map_err(|e| anyhow::anyhow!("OSS setup failed: {e}"))?;
    }

    let default_cwd = config.cwd.to_path_buf();
    let default_approval_policy = config.permissions.approval_policy.value();
    let default_sandbox_policy = config.permissions.sandbox_policy.get();
    let default_effort = config.model_reasoning_effort.clone();

    // When --yolo (dangerously_bypass_approvals_and_sandbox) is set, also skip the git repo check
    // since the user is explicitly running in an externally sandboxed environment.
    if !skip_git_repo_check
        && !dangerously_bypass_approvals_and_sandbox
        && get_git_repo_root(&default_cwd).is_none()
    {
        eprintln!("Not inside a trusted directory and --skip-git-repo-check was not specified.");
        std::process::exit(1);
    }

    let mut request_ids = RequestIdSequencer::new();
    let mut client = start_exec_app_gateway(in_process_start_args, remote_app_gateway).await?;

    // Handle resume subcommand through existing `thread/list` + `thread/resume`
    // APIs so exec no longer reaches into rollout storage directly.
    let (primary_thread_id, fallback_session_configured) =
        if let Some(ExecCommand::Resume(args)) = command.as_ref() {
            if let Some(thread_id) = resolve_resume_thread_id(&client, &config, args).await? {
                let response: ThreadResumeResponse = send_request_with_response(
                    &client,
                    ClientRequest::ThreadResume {
                        request_id: request_ids.next(),
                        params: thread_resume_params_from_config(&config, thread_id),
                    },
                    "thread/resume",
                )
                .await
                .map_err(anyhow::Error::msg)?;
                let session_configured = session_configured_from_thread_resume_response(&response)
                    .map_err(anyhow::Error::msg)?;
                (session_configured.session_id, session_configured)
            } else {
                let response: ThreadStartResponse = send_request_with_response(
                    &client,
                    ClientRequest::ThreadStart {
                        request_id: request_ids.next(),
                        params: thread_start_params_from_config(&config),
                    },
                    "thread/start",
                )
                .await
                .map_err(anyhow::Error::msg)?;
                let session_configured = session_configured_from_thread_start_response(&response)
                    .map_err(anyhow::Error::msg)?;
                (session_configured.session_id, session_configured)
            }
        } else {
            let response: ThreadStartResponse = send_request_with_response(
                &client,
                ClientRequest::ThreadStart {
                    request_id: request_ids.next(),
                    params: thread_start_params_from_config(&config),
                },
                "thread/start",
            )
            .await
            .map_err(anyhow::Error::msg)?;
            let session_configured = session_configured_from_thread_start_response(&response)
                .map_err(anyhow::Error::msg)?;
            (session_configured.session_id, session_configured)
        };

    let primary_thread_id_for_span = primary_thread_id.to_string();
    // Use the start/resume response as the authoritative bootstrap payload.
    // Waiting for a later streamed `SessionConfigured` event adds up to 10s of
    // avoidable startup latency on the in-process path.
    let session_configured = fallback_session_configured;

    exec_span.record("thread.id", primary_thread_id_for_span.as_str());

    let (initial_operation, prompt_summary) = match (command.as_ref(), prompt, images) {
        (Some(ExecCommand::Review(review_cli)), _, _) => {
            let review_request = build_review_request(review_cli)?;
            let summary = praxis_core::review_prompts::user_facing_hint(&review_request.target);
            (InitialOperation::Review { review_request }, summary)
        }
        (Some(ExecCommand::Resume(args)), root_prompt, imgs) => {
            let prompt_arg = args
                .prompt
                .clone()
                .or_else(|| {
                    if args.last {
                        args.session_id.clone()
                    } else {
                        None
                    }
                })
                .or(root_prompt);
            let prompt_text = resolve_prompt(prompt_arg);
            let mut items: Vec<UserInput> = imgs
                .into_iter()
                .chain(args.images.iter().cloned())
                .map(|path| UserInput::LocalImage { path })
                .collect();
            items.push(UserInput::Text {
                text: prompt_text.clone(),
                // CLI input doesn't track UI element ranges, so none are available here.
                text_elements: Vec::new(),
            });
            let output_schema = load_output_schema(output_schema_path.clone());
            (
                InitialOperation::UserTurn {
                    items,
                    output_schema,
                },
                prompt_text,
            )
        }
        (None, root_prompt, imgs) => {
            let prompt_text = resolve_root_prompt(root_prompt);
            let mut items: Vec<UserInput> = imgs
                .into_iter()
                .map(|path| UserInput::LocalImage { path })
                .collect();
            items.push(UserInput::Text {
                text: prompt_text.clone(),
                // CLI input doesn't track UI element ranges, so none are available here.
                text_elements: Vec::new(),
            });
            let output_schema = load_output_schema(output_schema_path);
            (
                InitialOperation::UserTurn {
                    items,
                    output_schema,
                },
                prompt_text,
            )
        }
    };

    // Print the effective configuration and initial request so users can see what Praxis
    // is using.
    event_processor.print_config_summary(&config, &prompt_summary, &session_configured);
    if !json_mode && let Some(message) = praxis_core::config::system_bwrap_warning() {
        event_processor.process_warning(message);
    }

    info!("Praxis initialized with event: {session_configured:?}");

    let (interrupt_tx, mut interrupt_rx) = mpsc::unbounded_channel::<()>();
    tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            tracing::debug!("Keyboard interrupt");
            let _ = interrupt_tx.send(());
        }
    });

    let task_id = match initial_operation {
        InitialOperation::UserTurn {
            items,
            output_schema,
        } => {
            let response: TurnStartResponse = send_request_with_response(
                &client,
                ClientRequest::TurnStart {
                    request_id: request_ids.next(),
                    params: TurnStartParams {
                        thread_id: primary_thread_id_for_span.clone(),
                        input: items.into_iter().map(Into::into).collect(),
                        cwd: Some(default_cwd),
                        approval_policy: Some(default_approval_policy.into()),
                        approvals_reviewer: None,
                        sandbox_policy: Some(default_sandbox_policy.clone().into()),
                        model_provider: None,
                        model: None,
                        service_tier: None,
                        effort: default_effort,
                        summary: None,
                        personality: None,
                        output_schema,
                        collaboration_mode: None,
                    },
                },
                "turn/start",
            )
            .await
            .map_err(anyhow::Error::msg)?;
            let task_id = response.turn.id;
            info!("Sent prompt with event ID: {task_id}");
            task_id
        }
        InitialOperation::Review { review_request } => {
            let response: ReviewStartResponse = send_request_with_response(
                &client,
                ClientRequest::ReviewStart {
                    request_id: request_ids.next(),
                    params: ReviewStartParams {
                        thread_id: primary_thread_id_for_span.clone(),
                        target: review_target_to_api(review_request.target),
                        delivery: None,
                    },
                },
                "review/start",
            )
            .await
            .map_err(anyhow::Error::msg)?;
            let _ = event_processor.process_server_notification(ServerNotification::TurnStarted(
                TurnStartedNotification {
                    thread_id: response.review_thread_id.clone(),
                    turn: response.turn.clone(),
                    model_context_window: None,
                },
            ));
            let task_id = response.turn.id;
            info!("Sent review request with event ID: {task_id}");
            task_id
        }
    };
    exec_span.record("turn.id", task_id.as_str());

    // Run the loop until the task is complete.
    // Track whether a fatal error was reported by the server so we can
    // exit with a non-zero status for automation-friendly signaling.
    let mut error_seen = false;
    let mut interrupt_channel_open = true;
    let primary_thread_id_for_requests = primary_thread_id.to_string();
    loop {
        let server_event = tokio::select! {
            maybe_interrupt = interrupt_rx.recv(), if interrupt_channel_open => {
                if maybe_interrupt.is_none() {
                    interrupt_channel_open = false;
                    continue;
                }
                if let Err(err) = send_request_with_response::<TurnInterruptResponse>(
                    &client,
                    ClientRequest::TurnInterrupt {
                        request_id: request_ids.next(),
                        params: TurnInterruptParams {
                            thread_id: primary_thread_id_for_requests.clone(),
                            turn_id: task_id.clone(),
                        },
                    },
                    "turn/interrupt",
                )
                .await
                {
                    warn!("turn/interrupt failed: {err}");
                }
                continue;
            }
            maybe_event = client.next_event() => maybe_event,
        };

        let Some(server_event) = server_event else {
            break;
        };

        match server_event {
            AppGatewayEvent::ServerRequest(request) => {
                handle_server_request(&client, request, &mut error_seen).await;
            }
            AppGatewayEvent::ServerNotification(mut notification) => {
                if let ServerNotification::Error(payload) = &notification {
                    if payload.thread_id == primary_thread_id_for_requests
                        && payload.turn_id == task_id
                        && !payload.will_retry
                    {
                        error_seen = true;
                    }
                } else if let ServerNotification::TurnCompleted(payload) = &notification
                    && payload.thread_id == primary_thread_id_for_requests
                    && payload.turn.id == task_id
                    && matches!(
                        payload.turn.status,
                        praxis_app_gateway_protocol::TurnStatus::Failed
                            | praxis_app_gateway_protocol::TurnStatus::Interrupted
                    )
                {
                    error_seen = true;
                }

                maybe_backfill_turn_completed_items(&client, &mut request_ids, &mut notification)
                    .await;

                if should_process_notification(
                    &notification,
                    &primary_thread_id_for_requests,
                    &task_id,
                ) {
                    match event_processor.process_server_notification(notification) {
                        PraxisStatus::Running => {}
                        PraxisStatus::InitiateShutdown => {
                            if let Err(err) = request_shutdown(
                                &client,
                                &mut request_ids,
                                &primary_thread_id_for_requests,
                            )
                            .await
                            {
                                warn!("thread/unsubscribe failed during shutdown: {err}");
                            }
                            break;
                        }
                    }
                }
            }
            AppGatewayEvent::Lagged { skipped } => {
                let message = lagged_event_warning_message(skipped);
                warn!("{message}");
                event_processor.process_warning(message);
            }
            AppGatewayEvent::Disconnected { message } => {
                warn!("{message}");
                event_processor.process_warning(message);
                error_seen = true;
                break;
            }
        }
    }

    if let Err(err) = client.shutdown().await {
        warn!("app-gateway client shutdown failed: {err}");
    }
    event_processor.print_final_output();
    if error_seen {
        std::process::exit(1);
    }

    Ok(())
}

fn sandbox_mode_from_policy(
    sandbox_policy: &praxis_protocol::protocol::SandboxPolicy,
) -> Option<praxis_app_gateway_protocol::SandboxMode> {
    match sandbox_policy {
        praxis_protocol::protocol::SandboxPolicy::DangerFullAccess => {
            Some(praxis_app_gateway_protocol::SandboxMode::DangerFullAccess)
        }
        praxis_protocol::protocol::SandboxPolicy::ReadOnly { .. } => {
            Some(praxis_app_gateway_protocol::SandboxMode::ReadOnly)
        }
        praxis_protocol::protocol::SandboxPolicy::WorkspaceWrite { .. } => {
            Some(praxis_app_gateway_protocol::SandboxMode::WorkspaceWrite)
        }
        praxis_protocol::protocol::SandboxPolicy::ExternalSandbox { .. } => None,
    }
}

fn thread_start_params_from_config(config: &Config) -> ThreadStartParams {
    ThreadStartParams {
        model: config.model.clone(),
        model_provider: Some(config.model_provider_id.clone()),
        cwd: Some(config.cwd.to_string_lossy().to_string()),
        approval_policy: Some(config.permissions.approval_policy.value().into()),
        approvals_reviewer: approvals_reviewer_override_from_config(config),
        sandbox: sandbox_mode_from_policy(config.permissions.sandbox_policy.get()),
        config: config_request_overrides_from_config(config),
        ephemeral: Some(config.ephemeral),
        ..ThreadStartParams::default()
    }
}

fn thread_resume_params_from_config(config: &Config, thread_id: String) -> ThreadResumeParams {
    ThreadResumeParams {
        thread_id,
        model: config.model.clone(),
        model_provider: Some(config.model_provider_id.clone()),
        cwd: Some(config.cwd.to_string_lossy().to_string()),
        approval_policy: Some(config.permissions.approval_policy.value().into()),
        approvals_reviewer: approvals_reviewer_override_from_config(config),
        sandbox: sandbox_mode_from_policy(config.permissions.sandbox_policy.get()),
        config: config_request_overrides_from_config(config),
        ..ThreadResumeParams::default()
    }
}

fn config_request_overrides_from_config(config: &Config) -> Option<HashMap<String, Value>> {
    config
        .active_profile
        .as_ref()
        .map(|profile| HashMap::from([("profile".to_string(), Value::String(profile.clone()))]))
}

fn approvals_reviewer_override_from_config(
    config: &Config,
) -> Option<praxis_app_gateway_protocol::ApprovalsReviewer> {
    Some(config.approvals_reviewer.into())
}

async fn send_request_with_response<T>(
    client: &AppGatewayClient,
    request: ClientRequest,
    method: &str,
) -> Result<T, String>
where
    T: serde::de::DeserializeOwned,
{
    client.request_typed(request).await.map_err(|err| {
        if method.is_empty() {
            err.to_string()
        } else {
            format!("{method}: {err}")
        }
    })
}

fn session_configured_from_thread_start_response(
    response: &ThreadStartResponse,
) -> Result<SessionConfiguredEvent, String> {
    session_configured_from_thread_response(
        &response.thread.id,
        response.thread.name.clone(),
        response.thread.path.clone(),
        response.model.clone(),
        response.model_provider.clone(),
        response.service_tier,
        response.approval_policy.to_core(),
        response.approvals_reviewer.to_core(),
        response.sandbox.to_core(),
        response.cwd.clone(),
        response.reasoning_effort.clone(),
    )
}

fn session_configured_from_thread_resume_response(
    response: &ThreadResumeResponse,
) -> Result<SessionConfiguredEvent, String> {
    session_configured_from_thread_response(
        &response.thread.id,
        response.thread.name.clone(),
        response.thread.path.clone(),
        response.model.clone(),
        response.model_provider.clone(),
        response.service_tier,
        response.approval_policy.to_core(),
        response.approvals_reviewer.to_core(),
        response.sandbox.to_core(),
        response.cwd.clone(),
        response.reasoning_effort.clone(),
    )
}

fn review_target_to_api(target: ReviewTarget) -> ApiReviewTarget {
    match target {
        ReviewTarget::UncommittedChanges => ApiReviewTarget::UncommittedChanges,
        ReviewTarget::BaseBranch { branch } => ApiReviewTarget::BaseBranch { branch },
        ReviewTarget::Commit { sha, title } => ApiReviewTarget::Commit { sha, title },
        ReviewTarget::Custom { instructions } => ApiReviewTarget::Custom { instructions },
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "session mapping keeps explicit fields"
)]
fn session_configured_from_thread_response(
    thread_id: &str,
    thread_name: Option<String>,
    rollout_path: Option<PathBuf>,
    model: String,
    model_provider_id: String,
    service_tier: Option<praxis_protocol::config_types::ServiceTier>,
    approval_policy: AskForApproval,
    approvals_reviewer: praxis_protocol::config_types::ApprovalsReviewer,
    sandbox_policy: SandboxPolicy,
    cwd: PathBuf,
    reasoning_effort: Option<praxis_protocol::openai_models::ReasoningEffort>,
) -> Result<SessionConfiguredEvent, String> {
    let session_id = praxis_protocol::ThreadId::from_string(thread_id)
        .map_err(|err| format!("thread id `{thread_id}` is invalid: {err}"))?;

    Ok(SessionConfiguredEvent {
        session_id,
        forked_from_id: None,
        thread_name,
        model,
        model_provider_id,
        service_tier,
        approval_policy,
        approvals_reviewer,
        sandbox_policy,
        cwd,
        reasoning_effort,
        history_log_id: 0,
        history_entry_count: 0,
        initial_messages: None,
        network_proxy: None,
        rollout_path,
    })
}
