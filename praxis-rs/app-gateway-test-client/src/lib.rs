use std::collections::VecDeque;
use std::ffi::OsString;
use std::fs;
use std::fs::OpenOptions;
use std::io::BufRead;
use std::io::BufReader;
use std::io::Write;
use std::net::TcpListener;
use std::net::TcpStream;
use std::path::Path;
use std::path::PathBuf;
use std::process::Child;
use std::process::ChildStdin;
use std::process::ChildStdout;
use std::process::Command;
use std::process::Stdio;
use std::thread;
use std::time::Duration;
use std::time::Instant;
use std::time::SystemTime;

use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use clap::ArgAction;
use clap::Parser;
use clap::Subcommand;
use praxis_app_gateway_protocol::AccountLoginCompletedNotification;
use praxis_app_gateway_protocol::AskForApproval;
use praxis_app_gateway_protocol::ClientInfo;
use praxis_app_gateway_protocol::ClientRequest;
use praxis_app_gateway_protocol::CommandExecutionApprovalDecision;
use praxis_app_gateway_protocol::CommandExecutionRequestApprovalParams;
use praxis_app_gateway_protocol::CommandExecutionRequestApprovalResponse;
use praxis_app_gateway_protocol::CommandExecutionStatus;
use praxis_app_gateway_protocol::DynamicToolSpec;
use praxis_app_gateway_protocol::FileChangeApprovalDecision;
use praxis_app_gateway_protocol::FileChangeRequestApprovalParams;
use praxis_app_gateway_protocol::FileChangeRequestApprovalResponse;
use praxis_app_gateway_protocol::GetAccountRateLimitsResponse;
use praxis_app_gateway_protocol::InitializeCapabilities;
use praxis_app_gateway_protocol::InitializeParams;
use praxis_app_gateway_protocol::InitializeResponse;
use praxis_app_gateway_protocol::JSONRPCMessage;
use praxis_app_gateway_protocol::JSONRPCNotification;
use praxis_app_gateway_protocol::JSONRPCRequest;
use praxis_app_gateway_protocol::JSONRPCResponse;
use praxis_app_gateway_protocol::LoginAccountResponse;
use praxis_app_gateway_protocol::ModelListParams;
use praxis_app_gateway_protocol::ModelListResponse;
use praxis_app_gateway_protocol::ReadOnlyAccess;
use praxis_app_gateway_protocol::RequestId;
use praxis_app_gateway_protocol::SandboxMode;
use praxis_app_gateway_protocol::SandboxPolicy;
use praxis_app_gateway_protocol::ServerNotification;
use praxis_app_gateway_protocol::ServerRequest;
use praxis_app_gateway_protocol::ThreadControlClaimParams;
use praxis_app_gateway_protocol::ThreadControlClaimResponse;
use praxis_app_gateway_protocol::ThreadControlReleaseParams;
use praxis_app_gateway_protocol::ThreadControlReleaseResponse;
use praxis_app_gateway_protocol::ThreadController;
use praxis_app_gateway_protocol::ThreadControllerKind;
use praxis_app_gateway_protocol::ThreadDecrementElicitationParams;
use praxis_app_gateway_protocol::ThreadDecrementElicitationResponse;
use praxis_app_gateway_protocol::ThreadIncrementElicitationParams;
use praxis_app_gateway_protocol::ThreadIncrementElicitationResponse;
use praxis_app_gateway_protocol::ThreadItem;
use praxis_app_gateway_protocol::ThreadListParams;
use praxis_app_gateway_protocol::ThreadListResponse;
use praxis_app_gateway_protocol::ThreadResumeParams;
use praxis_app_gateway_protocol::ThreadResumeResponse;
use praxis_app_gateway_protocol::ThreadStartParams;
use praxis_app_gateway_protocol::ThreadStartResponse;
use praxis_app_gateway_protocol::TurnStartParams;
use praxis_app_gateway_protocol::TurnStartResponse;
use praxis_app_gateway_protocol::TurnStatus;
use praxis_app_gateway_protocol::UserInput as ApiUserInput;
use praxis_core::config::Config;
use praxis_core::util::PRIMARY_CLI_COMMAND;
use praxis_otel::OtelProvider;
use praxis_otel::current_span_w3c_trace_context;
use praxis_protocol::openai_models::ReasoningEffort;
use praxis_protocol::protocol::W3cTraceContext;
use praxis_utils_cli::CliConfigOverrides;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use tracing::info_span;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tungstenite::Message;
use tungstenite::WebSocket;
use tungstenite::connect;
use tungstenite::stream::MaybeTlsStream;
use url::Url;
use uuid::Uuid;

#[path = "lib/client.rs"]
mod client;
#[path = "lib/commands.rs"]
mod commands;
#[path = "lib/endpoint.rs"]
mod endpoint;

use client::{
    CommandApprovalBehavior, PraxisClient, TestClientTracing, TraceSummary, print_trace_summary,
};
pub use commands::send_message_api;
use commands::{
    control_message_api, control_release, ensure_dynamic_tools_unused, get_account_rate_limits,
    live_elicitation_timeout_pause, model_list, no_trigger_cmd_approval, parse_dynamic_tools_arg,
    resume_message_api, send_follow_up_api, send_message, send_message_api_endpoint, test_login,
    thread_decrement_elicitation, thread_increment_elicitation, thread_list, thread_resume_follow,
    trigger_cmd_approval, trigger_patch_approval, trigger_zsh_fork_multi_cmd_approval, watch,
};
use endpoint::{
    BackgroundAppGateway, Endpoint, resolve_endpoint, resolve_shared_websocket_url, serve,
    shell_quote,
};

const NOTIFICATIONS_TO_OPT_OUT: &[&str] = &[
    // App-gateway item deltas.
    "command/exec/outputDelta",
    "item/agentMessage/delta",
    "item/plan/delta",
    "item/fileChange/outputDelta",
    "item/reasoning/summaryTextDelta",
    "item/reasoning/textDelta",
];
const APP_GATEWAY_GRACEFUL_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(5);
const APP_GATEWAY_GRACEFUL_SHUTDOWN_POLL_INTERVAL: Duration = Duration::from_millis(100);
const DEFAULT_ANALYTICS_ENABLED: bool = true;
const OTEL_SERVICE_NAME: &str = "praxis-app-gateway-test-client";
const TRACE_DISABLED_MESSAGE: &str =
    "Not enabled - enable tracing in $PRAXIS_HOME/config.toml to get a trace URL!";

/// Minimal launcher that initializes the Praxis app-gateway and logs the handshake.
#[derive(Parser)]
#[command(author = "Praxis", version, about = "Bootstrap Praxis app-gateway", long_about = None)]
struct Cli {
    /// Path to the `praxis` CLI binary. When set, requests use stdio by
    /// spawning `praxis app-gateway` as a child process.
    #[arg(long, env = "PRAXIS_BIN", global = true)]
    praxis_bin: Option<PathBuf>,

    /// Existing websocket server URL to connect to.
    ///
    /// If neither `--praxis-bin` nor `--url` is provided, defaults to
    /// `ws://127.0.0.1:4222`.
    #[arg(long, env = "PRAXIS_APP_GATEWAY_URL", global = true)]
    url: Option<String>,

    /// Forwarded to the `praxis` CLI as `--config key=value`. Repeatable.
    ///
    /// Example:
    ///   `--config 'model_providers.mock.base_url="http://localhost:4010/v1"'`
    #[arg(
        short = 'c',
        long = "config",
        value_name = "key=value",
        action = ArgAction::Append,
        global = true
    )]
    config_overrides: Vec<String>,

    /// JSON array of dynamic tool specs or a single tool object.
    /// Prefix a filename with '@' to read from a file.
    ///
    /// Example:
    ///   --dynamic-tools '[{"name":"demo","description":"Demo","inputSchema":{"type":"object"}}]'
    ///   --dynamic-tools @/path/to/tools.json
    #[arg(long, value_name = "json-or-@file", global = true)]
    dynamic_tools: Option<String>,

    #[command(subcommand)]
    command: CliCommand,
}

#[derive(Subcommand)]
enum CliCommand {
    /// Start `praxis app-gateway` on a websocket endpoint in the background.
    ///
    /// Logs are written to:
    ///   `/tmp/praxis-app-gateway-test-client/`
    Serve {
        /// WebSocket listen URL passed to `praxis app-gateway --listen`.
        #[arg(long, default_value = "ws://127.0.0.1:4222")]
        listen: String,
        /// Kill any process listening on the same port before starting.
        #[arg(long, default_value_t = false)]
        kill: bool,
    },
    /// Send a user message through the Praxis app-gateway.
    SendMessage {
        /// User message to send to Praxis.
        user_message: String,
    },
    /// Send a user message through the app-gateway thread/turn APIs.
    SendMessageApi {
        /// Opt into experimental app-gateway methods and fields.
        #[arg(long)]
        experimental_api: bool,
        /// User message to send to Praxis.
        user_message: String,
    },
    /// Acquire control of a new DeepSeek thread, then send a user message.
    ControlMessageApi {
        /// User message to send to the controlled DeepSeek thread.
        user_message: String,
        /// Keep the client alive after the turn completes so observers can inspect lock state.
        #[arg(long, default_value_t = 300)]
        hold_seconds: u64,
    },
    /// Release a Praxis harness control lock on a thread.
    ControlRelease {
        /// Existing thread id to unlock.
        thread_id: String,
    },
    /// Resume a thread by id, then send a user message.
    ResumeMessageApi {
        /// Existing thread id to resume.
        thread_id: String,
        /// User message to send to Praxis.
        user_message: String,
    },
    /// Resume a thread and continuously stream notifications/events.
    ///
    /// This command does not auto-exit; stop it with SIGINT/SIGTERM/SIGKILL.
    ThreadResume {
        /// Existing thread id to resume.
        thread_id: String,
    },
    /// Initialize the app-gateway and dump all inbound messages until interrupted.
    ///
    /// This command does not auto-exit; stop it with SIGINT/SIGTERM/SIGKILL.
    Watch,
    /// Start a turn that elicits an ExecCommand approval.
    #[command(name = "trigger-cmd-approval")]
    TriggerCmdApproval {
        /// Optional prompt; defaults to a simple python command.
        user_message: Option<String>,
    },
    /// Start a turn that elicits an ApplyPatch approval.
    #[command(name = "trigger-patch-approval")]
    TriggerPatchApproval {
        /// Optional prompt; defaults to creating a file via apply_patch.
        user_message: Option<String>,
    },
    /// Start a turn that should not elicit an ExecCommand approval.
    #[command(name = "no-trigger-cmd-approval")]
    NoTriggerCmdApproval,
    /// Send two sequential turns in the same thread to test follow-up behavior.
    SendFollowUpApi {
        /// Initial user message for the first turn.
        first_message: String,
        /// Follow-up user message for the second turn.
        follow_up_message: String,
    },
    /// Trigger zsh-fork multi-subcommand approvals and assert expected approval behavior.
    #[command(name = "trigger-zsh-fork-multi-cmd-approval")]
    TriggerZshForkMultiCmdApproval {
        /// Optional prompt; defaults to an explicit `/usr/bin/true && /usr/bin/true` command.
        user_message: Option<String>,
        /// Minimum number of command-approval callbacks expected in the turn.
        #[arg(long, default_value_t = 2)]
        min_approvals: usize,
        /// One-based approval index to abort (e.g. --abort-on 2 aborts the second approval).
        #[arg(long)]
        abort_on: Option<usize>,
    },
    /// Trigger the ChatGPT login flow and wait for completion.
    TestLogin {
        /// Use the device-code login flow instead of the browser callback flow.
        #[arg(long, default_value_t = false)]
        device_code: bool,
    },
    /// Fetch the current account rate limits from the Praxis app-gateway.
    GetAccountRateLimits,
    /// List the available models from the Praxis app-gateway.
    #[command(name = "model-list")]
    ModelList,
    /// List stored threads from the Praxis app-gateway.
    #[command(name = "thread-list")]
    ThreadList {
        /// Number of threads to return.
        #[arg(long, default_value_t = 20)]
        limit: u32,
    },
    /// Increment the out-of-band elicitation pause counter for a thread.
    #[command(name = "thread-increment-elicitation")]
    ThreadIncrementElicitation {
        /// Existing thread id to update.
        thread_id: String,
    },
    /// Decrement the out-of-band elicitation pause counter for a thread.
    #[command(name = "thread-decrement-elicitation")]
    ThreadDecrementElicitation {
        /// Existing thread id to update.
        thread_id: String,
    },
    /// Run the live websocket harness that proves elicitation pause prevents a
    /// 10s unified exec timeout from killing a 15s helper script.
    #[command(name = "live-elicitation-timeout-pause")]
    LiveElicitationTimeoutPause {
        /// Model passed to `thread/start`.
        #[arg(long, env = "PRAXIS_E2E_MODEL", default_value = "gpt-5")]
        model: String,
        /// Existing workspace path used as the turn cwd.
        #[arg(long, value_name = "path", default_value = ".")]
        workspace: PathBuf,
        /// Helper script to run from the model; defaults to the repo-local
        /// live elicitation hold script.
        #[arg(long, value_name = "path")]
        script: Option<PathBuf>,
        /// Seconds the helper script should sleep while the timeout is paused.
        #[arg(long, default_value_t = 15)]
        hold_seconds: u64,
    },
}

pub async fn run() -> Result<()> {
    let Cli {
        praxis_bin,
        url,
        config_overrides,
        dynamic_tools,
        command,
    } = Cli::parse();

    let dynamic_tools = parse_dynamic_tools_arg(&dynamic_tools)?;

    match command {
        CliCommand::Serve { listen, kill } => {
            ensure_dynamic_tools_unused(&dynamic_tools, "serve")?;
            let praxis_bin = praxis_bin.unwrap_or_else(|| PathBuf::from(PRIMARY_CLI_COMMAND));
            serve(&praxis_bin, &config_overrides, &listen, kill)
        }
        CliCommand::SendMessage { user_message } => {
            ensure_dynamic_tools_unused(&dynamic_tools, "send-message")?;
            let endpoint = resolve_endpoint(praxis_bin, url)?;
            send_message(&endpoint, &config_overrides, user_message).await
        }
        CliCommand::SendMessageApi {
            experimental_api,
            user_message,
        } => {
            let endpoint = resolve_endpoint(praxis_bin, url)?;
            send_message_api_endpoint(
                &endpoint,
                &config_overrides,
                user_message,
                experimental_api,
                &dynamic_tools,
            )
            .await
        }
        CliCommand::ControlMessageApi {
            user_message,
            hold_seconds,
        } => {
            ensure_dynamic_tools_unused(&dynamic_tools, "control-message-api")?;
            let endpoint = resolve_endpoint(praxis_bin, url)?;
            control_message_api(&endpoint, &config_overrides, user_message, hold_seconds).await
        }
        CliCommand::ControlRelease { thread_id } => {
            ensure_dynamic_tools_unused(&dynamic_tools, "control-release")?;
            let endpoint = resolve_endpoint(praxis_bin, url)?;
            control_release(&endpoint, &config_overrides, thread_id).await
        }
        CliCommand::ResumeMessageApi {
            thread_id,
            user_message,
        } => {
            let endpoint = resolve_endpoint(praxis_bin, url)?;
            resume_message_api(
                &endpoint,
                &config_overrides,
                thread_id,
                user_message,
                &dynamic_tools,
            )
            .await
        }
        CliCommand::ThreadResume { thread_id } => {
            ensure_dynamic_tools_unused(&dynamic_tools, "thread-resume")?;
            let endpoint = resolve_endpoint(praxis_bin, url)?;
            thread_resume_follow(&endpoint, &config_overrides, thread_id).await
        }
        CliCommand::Watch => {
            ensure_dynamic_tools_unused(&dynamic_tools, "watch")?;
            let endpoint = resolve_endpoint(praxis_bin, url)?;
            watch(&endpoint, &config_overrides).await
        }
        CliCommand::TriggerCmdApproval { user_message } => {
            let endpoint = resolve_endpoint(praxis_bin, url)?;
            trigger_cmd_approval(&endpoint, &config_overrides, user_message, &dynamic_tools).await
        }
        CliCommand::TriggerPatchApproval { user_message } => {
            let endpoint = resolve_endpoint(praxis_bin, url)?;
            trigger_patch_approval(&endpoint, &config_overrides, user_message, &dynamic_tools).await
        }
        CliCommand::NoTriggerCmdApproval => {
            let endpoint = resolve_endpoint(praxis_bin, url)?;
            no_trigger_cmd_approval(&endpoint, &config_overrides, &dynamic_tools).await
        }
        CliCommand::SendFollowUpApi {
            first_message,
            follow_up_message,
        } => {
            let endpoint = resolve_endpoint(praxis_bin, url)?;
            send_follow_up_api(
                &endpoint,
                &config_overrides,
                first_message,
                follow_up_message,
                &dynamic_tools,
            )
            .await
        }
        CliCommand::TriggerZshForkMultiCmdApproval {
            user_message,
            min_approvals,
            abort_on,
        } => {
            let endpoint = resolve_endpoint(praxis_bin, url)?;
            trigger_zsh_fork_multi_cmd_approval(
                &endpoint,
                &config_overrides,
                user_message,
                min_approvals,
                abort_on,
                &dynamic_tools,
            )
            .await
        }
        CliCommand::TestLogin { device_code } => {
            ensure_dynamic_tools_unused(&dynamic_tools, "test-login")?;
            let endpoint = resolve_endpoint(praxis_bin, url)?;
            test_login(&endpoint, &config_overrides, device_code).await
        }
        CliCommand::GetAccountRateLimits => {
            ensure_dynamic_tools_unused(&dynamic_tools, "get-account-rate-limits")?;
            let endpoint = resolve_endpoint(praxis_bin, url)?;
            get_account_rate_limits(&endpoint, &config_overrides).await
        }
        CliCommand::ModelList => {
            ensure_dynamic_tools_unused(&dynamic_tools, "model-list")?;
            let endpoint = resolve_endpoint(praxis_bin, url)?;
            model_list(&endpoint, &config_overrides).await
        }
        CliCommand::ThreadList { limit } => {
            ensure_dynamic_tools_unused(&dynamic_tools, "thread-list")?;
            let endpoint = resolve_endpoint(praxis_bin, url)?;
            thread_list(&endpoint, &config_overrides, limit).await
        }
        CliCommand::ThreadIncrementElicitation { thread_id } => {
            ensure_dynamic_tools_unused(&dynamic_tools, "thread-increment-elicitation")?;
            let url =
                resolve_shared_websocket_url(praxis_bin, url, "thread-increment-elicitation")?;
            thread_increment_elicitation(&url, thread_id)
        }
        CliCommand::ThreadDecrementElicitation { thread_id } => {
            ensure_dynamic_tools_unused(&dynamic_tools, "thread-decrement-elicitation")?;
            let url =
                resolve_shared_websocket_url(praxis_bin, url, "thread-decrement-elicitation")?;
            thread_decrement_elicitation(&url, thread_id)
        }
        CliCommand::LiveElicitationTimeoutPause {
            model,
            workspace,
            script,
            hold_seconds,
        } => {
            ensure_dynamic_tools_unused(&dynamic_tools, "live-elicitation-timeout-pause")?;
            live_elicitation_timeout_pause(
                praxis_bin,
                url,
                &config_overrides,
                model,
                workspace,
                script,
                hold_seconds,
            )
        }
    }
}
