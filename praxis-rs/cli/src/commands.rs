#[cfg(target_os = "macos")]
use crate::app_cmd;
use crate::feature_flags::FeatureToggles;
use crate::feature_flags::FeaturesCli;
use crate::mcp_cmd::McpCli;
use crate::remote_control::InteractiveRemoteOptions;
use clap::Args;
use clap::Parser;
use clap_complete::Shell;
use praxis_app_gateway_service as praxis_app_gateway;
use praxis_chatgpt::apply_command::ApplyCommand;
use praxis_cli::LandlockCommand;
use praxis_cli::SeatbeltCommand;
use praxis_cli::WindowsCommand;
use praxis_cloud_tasks::Cli as CloudTasksCli;
use praxis_exec::Cli as ExecCli;
use praxis_exec::ReviewArgs;
use praxis_execpolicy::ExecPolicyCheckCommand;
use praxis_responses_api_proxy::Args as ResponsesApiProxyArgs;
use praxis_tui::Cli as TuiCli;
use praxis_utils_cli::CliConfigOverrides;
use std::path::PathBuf;
/// Praxis CLI
///
/// If no subcommand is specified, Praxis opens the workspace.
#[derive(Debug, Parser)]
#[clap(
    name = "praxis",
    author,
    version,
    // If a sub‑command is given, ignore requirements of the default args.
    subcommand_negates_reqs = true,
    // The executable is sometimes invoked via a platform‑specific name like
    // `praxis-x86_64-unknown-linux-musl`, but the help output should always use
    // the generic `praxis` command name that users run.
    bin_name = "praxis",
    override_usage = "praxis [OPTIONS] [PROMPT]\n       praxis [OPTIONS] <COMMAND> [ARGS]"
)]
pub(crate) struct MultitoolCli {
    #[clap(flatten)]
    pub(crate) config_overrides: CliConfigOverrides,

    #[clap(flatten)]
    pub(crate) feature_toggles: FeatureToggles,

    #[clap(flatten)]
    pub(crate) remote: InteractiveRemoteOptions,

    #[clap(flatten)]
    pub(crate) interactive: TuiCli,

    #[clap(subcommand)]
    pub(crate) subcommand: Option<Subcommand>,
}

#[derive(Debug, clap::Subcommand)]
pub(crate) enum Subcommand {
    /// Run Praxis non-interactively.
    #[clap(visible_alias = "e")]
    Exec(ExecCli),

    /// Run a code review non-interactively.
    Review(ReviewArgs),

    /// Manage login.
    Login(LoginCommand),

    /// Remove stored authentication credentials.
    Logout(LogoutCommand),

    /// Manage external MCP servers for Praxis.
    Mcp(McpCli),

    /// Start Praxis as an MCP server (stdio).
    McpServer,

    /// [internal] Run the app gateway or related tooling.
    #[clap(hide = true)]
    AppGateway(AppGatewayCommand),

    /// Developer-only legacy single-thread TUI.
    #[clap(hide = true)]
    Dev(DevCommand),

    /// Launch the Praxis desktop app (downloads the macOS installer if missing).
    #[cfg(target_os = "macos")]
    App(app_cmd::AppCommand),

    /// Generate shell completion scripts.
    Completion(CompletionCommand),

    /// Run commands within a Praxis-provided sandbox.
    Sandbox(SandboxArgs),

    /// Debugging tools.
    Debug(DebugCommand),

    /// Execpolicy tooling.
    #[clap(hide = true)]
    Execpolicy(ExecpolicyCommand),

    /// Apply the latest diff produced by Praxis agent as a `git apply` to your local working tree.
    #[clap(visible_alias = "a")]
    Apply(ApplyCommand),

    /// Resume a previous interactive session (picker by default; use --last to continue the most recent).
    Resume(ResumeCommand),

    /// Fork a previous interactive session (picker by default; use --last to fork the most recent).
    Fork(ForkCommand),

    /// [EXPERIMENTAL] Browse cloud tasks and apply changes locally.
    #[clap(name = "cloud", alias = "cloud-tasks")]
    Cloud(CloudTasksCli),

    /// Internal: run the responses API proxy.
    #[clap(hide = true)]
    ResponsesApiProxy(ResponsesApiProxyArgs),

    /// Internal: relay stdio to a Unix domain socket.
    #[clap(hide = true, name = "stdio-to-uds")]
    StdioToUds(StdioToUdsCommand),

    /// Inspect feature flags.
    Features(FeaturesCli),
}

#[derive(Debug, Parser)]
pub(crate) struct CompletionCommand {
    /// Shell to generate completions for
    #[clap(value_enum, default_value_t = Shell::Bash)]
    pub(crate) shell: Shell,
}

#[derive(Debug, Parser)]
pub(crate) struct DebugCommand {
    #[command(subcommand)]
    pub(crate) subcommand: DebugSubcommand,
}

#[derive(Debug, clap::Subcommand)]
pub(crate) enum DebugSubcommand {
    /// Tooling: helps debug the app gateway.
    AppGateway(DebugAppGatewayCommand),

    /// Tooling: run Praxis web_search directly and print the structured result.
    WebSearch(DebugWebSearchCommand),

    /// Internal: reset local memory state for a fresh start.
    #[clap(hide = true)]
    ClearMemories,
}

#[derive(Debug, Parser)]
pub(crate) struct DebugWebSearchCommand {
    #[arg(value_name = "QUERY", required = true)]
    pub(crate) query: String,

    #[arg(long, default_value_t = 10)]
    pub(crate) max_results: usize,

    #[arg(long = "domain", value_name = "DOMAIN")]
    pub(crate) domains: Vec<String>,

    #[arg(long)]
    pub(crate) recency_days: Option<u32>,
}

#[derive(Debug, Parser)]
pub(crate) struct DebugAppGatewayCommand {
    #[command(subcommand)]
    pub(crate) subcommand: DebugAppGatewaySubcommand,
}

#[derive(Debug, clap::Subcommand)]
pub(crate) enum DebugAppGatewaySubcommand {
    // Send a message through the app-gateway canonical API.
    SendMessageApi(DebugAppGatewaySendMessageApiCommand),
}

#[derive(Debug, Parser)]
pub(crate) struct DebugAppGatewaySendMessageApiCommand {
    #[arg(value_name = "USER_MESSAGE", required = true)]
    pub(crate) user_message: String,
}

#[derive(Debug, Parser)]
pub(crate) struct ResumeCommand {
    /// Optional source namespace (`praxis`, `codex`, or `cursor`) followed by a conversation/session id
    /// (UUID) or thread name. If omitted, defaults to Praxis sessions.
    #[arg(value_name = "SOURCE_OR_TARGET")]
    pub(crate) target: Option<String>,

    /// Optional conversation/session id or thread name when the first argument is a source.
    #[arg(value_name = "TARGET")]
    pub(crate) target_extra: Option<String>,

    /// Continue the most recent session without showing the picker.
    #[arg(long = "last", default_value_t = false)]
    pub(crate) last: bool,

    /// Show all sessions (disables cwd filtering and shows CWD column).
    #[arg(long = "all", default_value_t = false)]
    pub(crate) all: bool,

    /// Include non-interactive sessions in the resume picker and --last selection.
    #[arg(long = "include-non-interactive", default_value_t = false)]
    pub(crate) include_non_interactive: bool,

    #[clap(flatten)]
    pub(crate) remote: InteractiveRemoteOptions,

    #[clap(flatten)]
    pub(crate) config_overrides: TuiCli,
}

#[derive(Debug, Parser)]
pub(crate) struct ForkCommand {
    /// Optional source namespace (`praxis`, `codex`, or `cursor`) followed by a conversation/session id
    /// (UUID) or thread name. If omitted, defaults to Praxis sessions.
    #[arg(value_name = "SOURCE_OR_TARGET")]
    pub(crate) target: Option<String>,

    /// Optional conversation/session id or thread name when the first argument is a source.
    #[arg(value_name = "TARGET")]
    pub(crate) target_extra: Option<String>,

    /// Fork the most recent session without showing the picker.
    #[arg(long = "last", default_value_t = false)]
    pub(crate) last: bool,

    /// Show all sessions (disables cwd filtering and shows CWD column).
    #[arg(long = "all", default_value_t = false)]
    pub(crate) all: bool,

    #[clap(flatten)]
    pub(crate) remote: InteractiveRemoteOptions,

    #[clap(flatten)]
    pub(crate) config_overrides: TuiCli,
}

#[derive(Debug, Parser)]
pub(crate) struct SandboxArgs {
    #[command(subcommand)]
    pub(crate) cmd: SandboxCommand,
}

#[derive(Debug, clap::Subcommand)]
pub(crate) enum SandboxCommand {
    /// Run a command under Seatbelt (macOS only).
    #[clap(visible_alias = "seatbelt")]
    Macos(SeatbeltCommand),

    /// Run a command under the Linux sandbox (bubblewrap by default).
    #[clap(visible_alias = "landlock")]
    Linux(LandlockCommand),

    /// Run a command under Windows restricted token (Windows only).
    Windows(WindowsCommand),
}

#[derive(Debug, Parser)]
pub(crate) struct ExecpolicyCommand {
    #[command(subcommand)]
    pub(crate) sub: ExecpolicySubcommand,
}

#[derive(Debug, clap::Subcommand)]
pub(crate) enum ExecpolicySubcommand {
    /// Check execpolicy files against a command.
    #[clap(name = "check")]
    Check(ExecPolicyCheckCommand),
}

#[derive(Debug, Parser)]
pub(crate) struct LoginCommand {
    #[clap(skip)]
    pub(crate) config_overrides: CliConfigOverrides,

    #[arg(
        long = "with-api-key",
        help = "Read the API key from stdin (e.g. `printenv OPENAI_API_KEY | praxis login --with-api-key`)"
    )]
    pub(crate) with_api_key: bool,

    #[arg(
        long = "api-key",
        value_name = "API_KEY",
        help = "(deprecated) Previously accepted the API key directly; now exits with guidance to use --with-api-key",
        hide = true
    )]
    pub(crate) api_key: Option<String>,

    #[arg(long = "device-auth")]
    pub(crate) use_device_code: bool,

    /// EXPERIMENTAL: Use custom OAuth issuer base URL (advanced)
    /// Override the OAuth issuer base URL (advanced)
    #[arg(long = "experimental_issuer", value_name = "URL", hide = true)]
    pub(crate) issuer_base_url: Option<String>,

    /// EXPERIMENTAL: Use custom OAuth client ID (advanced)
    #[arg(long = "experimental_client-id", value_name = "CLIENT_ID", hide = true)]
    pub(crate) client_id: Option<String>,

    #[command(subcommand)]
    pub(crate) action: Option<LoginSubcommand>,
}

#[derive(Debug, clap::Subcommand)]
pub(crate) enum LoginSubcommand {
    /// Show login status.
    Status,
}

#[derive(Debug, Parser)]
pub(crate) struct LogoutCommand {
    #[clap(skip)]
    pub(crate) config_overrides: CliConfigOverrides,
}

#[derive(Debug, Parser)]
pub(crate) struct DevCommand {
    #[clap(flatten)]
    pub(crate) interactive: TuiCli,
}

#[derive(Debug, Parser)]
pub(crate) struct AppGatewayCommand {
    /// Omit to run the app gateway; specify a subcommand for tooling.
    #[command(subcommand)]
    pub(crate) subcommand: Option<AppGatewaySubcommand>,

    /// Transport endpoint URL. Supported values: `stdio://` (default),
    /// `ws://IP:PORT`.
    #[arg(
        long = "listen",
        value_name = "URL",
        default_value = praxis_app_gateway::AppGatewayTransport::DEFAULT_LISTEN_URL
    )]
    pub(crate) listen: praxis_app_gateway::AppGatewayTransport,

    /// Controls whether analytics are enabled by default.
    ///
    /// Analytics are disabled by default for app-gateway. Users have to explicitly opt in
    /// via the `analytics` section in the config.toml file.
    ///
    /// However, for first-party use cases like the VSCode IDE extension, we default analytics
    /// to be enabled by default by setting this flag. Users can still opt out by setting this
    /// in their config.toml:
    ///
    /// ```toml
    /// [analytics]
    /// enabled = false
    /// ```
    ///
    /// See the local configuration documentation for more details.
    #[arg(long = "analytics-default-enabled")]
    pub(crate) analytics_default_enabled: bool,

    #[command(flatten)]
    pub(crate) auth: praxis_app_gateway::AppGatewayWebsocketAuthArgs,
}

#[derive(Debug, clap::Subcommand)]
#[allow(clippy::enum_variant_names)]
pub(crate) enum AppGatewaySubcommand {
    /// [experimental] Generate TypeScript bindings for the app gateway protocol.
    GenerateTs(GenerateTsCommand),

    /// [experimental] Generate JSON Schema for the app gateway protocol.
    GenerateJsonSchema(GenerateJsonSchemaCommand),

    /// [internal] Generate internal JSON Schema artifacts for Praxis tooling.
    #[clap(hide = true)]
    GenerateInternalJsonSchema(GenerateInternalJsonSchemaCommand),
}

#[derive(Debug, Args)]
pub(crate) struct GenerateTsCommand {
    /// Output directory where .ts files will be written
    #[arg(short = 'o', long = "out", value_name = "DIR")]
    pub(crate) out_dir: PathBuf,

    /// Optional path to the Prettier executable to format generated files
    #[arg(short = 'p', long = "prettier", value_name = "PRETTIER_BIN")]
    pub(crate) prettier: Option<PathBuf>,

    /// Include experimental methods and fields in the generated output
    #[arg(long = "experimental", default_value_t = false)]
    pub(crate) experimental: bool,
}

#[derive(Debug, Args)]
pub(crate) struct GenerateJsonSchemaCommand {
    /// Output directory where the schema bundle will be written
    #[arg(short = 'o', long = "out", value_name = "DIR")]
    pub(crate) out_dir: PathBuf,

    /// Include experimental methods and fields in the generated output
    #[arg(long = "experimental", default_value_t = false)]
    pub(crate) experimental: bool,
}

#[derive(Debug, Args)]
pub(crate) struct GenerateInternalJsonSchemaCommand {
    /// Output directory where internal JSON Schema artifacts will be written
    #[arg(short = 'o', long = "out", value_name = "DIR")]
    pub(crate) out_dir: PathBuf,
}

#[derive(Debug, Parser)]
pub(crate) struct StdioToUdsCommand {
    /// Path to the Unix domain socket to connect to.
    #[arg(value_name = "SOCKET_PATH")]
    pub(crate) socket_path: PathBuf,
}
