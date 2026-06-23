use std::collections::BTreeSet;
use std::collections::HashMap;
use std::future::Future;
use std::io;
use std::path::Path;
use std::path::PathBuf;
use std::pin::Pin;
use std::process::ExitStatus;
use std::time::Duration;
use std::time::Instant;

use async_channel::Sender;
use tokio_util::sync::CancellationToken;

use crate::error::PraxisErr;
use crate::error::Result;
use crate::error::SandboxErr;
use crate::sandboxing::ExecOptions;
use crate::sandboxing::ExecRequest;
use crate::sandboxing::SandboxPermissions;
use crate::spawn::SpawnChildRequest;
use crate::spawn::StdioPolicy;
use crate::spawn::spawn_child_async;
use praxis_network_proxy::NetworkProxy;
use praxis_protocol::config_types::WindowsSandboxLevel;
use praxis_protocol::permissions::FileSystemSandboxKind;
use praxis_protocol::permissions::FileSystemSandboxPolicy;
use praxis_protocol::permissions::NetworkSandboxPolicy;
use praxis_protocol::protocol::Event;
use praxis_protocol::protocol::SandboxPolicy;
use praxis_sandboxing::SandboxCommand;
use praxis_sandboxing::SandboxManager;
use praxis_sandboxing::SandboxTransformRequest;
use praxis_sandboxing::SandboxType;
use praxis_sandboxing::SandboxablePreference;
use praxis_utils_absolute_path::AbsolutePathBuf;
use praxis_utils_pty::DEFAULT_OUTPUT_BYTES_CAP;

mod output;

pub use output::ExecOutputSpool;
pub use output::ExecStreamSpool;
pub use output::ExecToolCallOutput;
use output::RawExecToolCallOutput;
pub use output::StreamOutput;
#[cfg(any(target_os = "windows", test))]
use output::aggregate_output;
use output::consume_output;
#[cfg(test)]
use output::read_output;
#[cfg(target_os = "windows")]
use output::write_capture_output_spool;

pub const DEFAULT_EXEC_COMMAND_TIMEOUT_MS: u64 = 10_000;

// Hardcode these since it does not seem worth including the libc crate just
// for these.
const SIGKILL_CODE: i32 = 9;
const TIMEOUT_CODE: i32 = 64;
const EXIT_CODE_SIGNAL_BASE: i32 = 128; // conventional shell: 128 + signal
const EXEC_TIMEOUT_EXIT_CODE: i32 = 124; // conventional timeout exit code

/// Hard cap on bytes retained from exec stdout/stderr/aggregated output.
///
/// This mirrors unified exec's output cap so a single runaway command cannot
/// OOM the process by dumping huge amounts of data to stdout/stderr.
const EXEC_OUTPUT_MAX_BYTES: usize = DEFAULT_OUTPUT_BYTES_CAP;

/// Limit the number of ExecCommandOutputDelta events emitted per exec call.
/// Aggregation still collects full output; only the live event stream is capped.
pub(crate) const MAX_EXEC_OUTPUT_DELTAS_PER_CALL: usize = 10_000;

// Wait for the stdout/stderr collection tasks but guard against them
// hanging forever. In the normal case, both pipes are closed once the child
// terminates so the tasks exit quickly. However, if the child process
// spawned grandchildren that inherited its stdout/stderr file descriptors
// those pipes may stay open after we `kill` the direct child on timeout.
// That would cause the `read_capped` tasks to block on `read()`
// indefinitely, effectively hanging the whole agent.
pub const IO_DRAIN_TIMEOUT_MS: u64 = 2_000; // 2 s should be plenty for local pipes

#[derive(Debug)]
pub struct ExecParams {
    pub command: Vec<String>,
    pub cwd: PathBuf,
    pub expiration: ExecExpiration,
    pub capture_policy: ExecCapturePolicy,
    pub env: HashMap<String, String>,
    pub network: Option<NetworkProxy>,
    pub sandbox_permissions: SandboxPermissions,
    pub windows_sandbox_level: praxis_protocol::config_types::WindowsSandboxLevel,
    pub windows_sandbox_private_desktop: bool,
    pub justification: Option<String>,
    pub arg0: Option<String>,
}

/// Extra filesystem deny-write carveouts for the non-elevated Windows
/// restricted-token backend.
///
/// These are applied on top of the legacy `WorkspaceWrite` allow set, so we
/// can support a narrow split-policy subset without changing legacy Windows
/// sandbox semantics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WindowsRestrictedTokenFilesystemOverlay {
    pub(crate) additional_deny_write_paths: Vec<AbsolutePathBuf>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ExecCapturePolicy {
    /// Shell-like execs keep the historical output cap and timeout behavior.
    #[default]
    ShellTool,
    /// Trusted internal helpers can buffer the full child output in memory
    /// without the shell-oriented output cap or exec-expiration behavior.
    FullBuffer,
}

fn select_process_exec_tool_sandbox_type(
    file_system_sandbox_policy: &FileSystemSandboxPolicy,
    network_sandbox_policy: NetworkSandboxPolicy,
    windows_sandbox_level: praxis_protocol::config_types::WindowsSandboxLevel,
    enforce_managed_network: bool,
) -> SandboxType {
    SandboxManager::new().select_initial(
        file_system_sandbox_policy,
        network_sandbox_policy,
        SandboxablePreference::Auto,
        windows_sandbox_level,
        enforce_managed_network,
    )
}

/// Mechanism to terminate an exec invocation before it finishes naturally.
#[derive(Clone, Debug)]
pub enum ExecExpiration {
    Timeout(Duration),
    DefaultTimeout,
    Cancellation(CancellationToken),
}

impl From<Option<u64>> for ExecExpiration {
    fn from(timeout_ms: Option<u64>) -> Self {
        timeout_ms.map_or(ExecExpiration::DefaultTimeout, |timeout_ms| {
            ExecExpiration::Timeout(Duration::from_millis(timeout_ms))
        })
    }
}

impl From<u64> for ExecExpiration {
    fn from(timeout_ms: u64) -> Self {
        ExecExpiration::Timeout(Duration::from_millis(timeout_ms))
    }
}

impl ExecExpiration {
    pub(crate) async fn wait(self) {
        match self {
            ExecExpiration::Timeout(duration) => tokio::time::sleep(duration).await,
            ExecExpiration::DefaultTimeout => {
                tokio::time::sleep(Duration::from_millis(DEFAULT_EXEC_COMMAND_TIMEOUT_MS)).await
            }
            ExecExpiration::Cancellation(cancel) => {
                cancel.cancelled().await;
            }
        }
    }

    /// If ExecExpiration is a timeout, returns the timeout in milliseconds.
    pub(crate) fn timeout_ms(&self) -> Option<u64> {
        match self {
            ExecExpiration::Timeout(duration) => Some(duration.as_millis() as u64),
            ExecExpiration::DefaultTimeout => Some(DEFAULT_EXEC_COMMAND_TIMEOUT_MS),
            ExecExpiration::Cancellation(_) => None,
        }
    }
}

impl ExecCapturePolicy {
    fn retained_bytes_cap(self) -> Option<usize> {
        match self {
            Self::ShellTool => Some(EXEC_OUTPUT_MAX_BYTES),
            Self::FullBuffer => None,
        }
    }

    fn io_drain_timeout(self) -> Duration {
        Duration::from_millis(IO_DRAIN_TIMEOUT_MS)
    }

    fn uses_expiration(self) -> bool {
        match self {
            Self::ShellTool => true,
            Self::FullBuffer => false,
        }
    }
}

#[derive(Clone)]
pub struct StdoutStream {
    pub sub_id: String,
    pub call_id: String,
    pub tx_event: Sender<Event>,
}

pub(crate) type SpawnObserver =
    Box<dyn FnOnce(Option<u32>) -> Pin<Box<dyn Future<Output = ()> + Send>> + Send>;

#[allow(clippy::too_many_arguments)]
pub async fn process_exec_tool_call(
    params: ExecParams,
    sandbox_policy: &SandboxPolicy,
    file_system_sandbox_policy: &FileSystemSandboxPolicy,
    network_sandbox_policy: NetworkSandboxPolicy,
    sandbox_cwd: &Path,
    praxis_linux_sandbox_exe: &Option<PathBuf>,
    use_legacy_landlock: bool,
    stdout_stream: Option<StdoutStream>,
) -> Result<ExecToolCallOutput> {
    let exec_req = build_exec_request(
        params,
        sandbox_policy,
        file_system_sandbox_policy,
        network_sandbox_policy,
        sandbox_cwd,
        praxis_linux_sandbox_exe,
        use_legacy_landlock,
    )?;

    // Route through the sandboxing module for a single, unified execution path.
    crate::sandboxing::execute_env(exec_req, stdout_stream).await
}

/// Transform a portable exec request into the concrete argv/env that should be
/// spawned under the requested sandbox policy.
pub fn build_exec_request(
    params: ExecParams,
    sandbox_policy: &SandboxPolicy,
    file_system_sandbox_policy: &FileSystemSandboxPolicy,
    network_sandbox_policy: NetworkSandboxPolicy,
    sandbox_cwd: &Path,
    praxis_linux_sandbox_exe: &Option<PathBuf>,
    use_legacy_landlock: bool,
) -> Result<ExecRequest> {
    let windows_sandbox_level = params.windows_sandbox_level;
    let enforce_managed_network = params.network.is_some();
    let sandbox_type = select_process_exec_tool_sandbox_type(
        file_system_sandbox_policy,
        network_sandbox_policy,
        windows_sandbox_level,
        enforce_managed_network,
    );
    tracing::debug!("Sandbox type: {sandbox_type:?}");

    let ExecParams {
        command,
        cwd,
        mut env,
        expiration,
        capture_policy,
        network,
        sandbox_permissions: _,
        windows_sandbox_level,
        windows_sandbox_private_desktop,
        justification: _,
        arg0: _,
    } = params;
    if let Some(network) = network.as_ref() {
        network.apply_to_env(&mut env);
    }
    let (program, args) = command.split_first().ok_or_else(|| {
        PraxisErr::Io(io::Error::new(
            io::ErrorKind::InvalidInput,
            "command args are empty",
        ))
    })?;

    let manager = SandboxManager::new();
    let command = SandboxCommand {
        program: program.clone().into(),
        args: args.to_vec(),
        cwd,
        env,
        additional_permissions: None,
    };
    let options = ExecOptions {
        expiration,
        capture_policy,
    };
    let mut exec_req = manager
        .transform(SandboxTransformRequest {
            command,
            policy: sandbox_policy,
            file_system_policy: file_system_sandbox_policy,
            network_policy: network_sandbox_policy,
            sandbox: sandbox_type,
            enforce_managed_network,
            network: network.as_ref(),
            sandbox_policy_cwd: sandbox_cwd,
            praxis_linux_sandbox_exe: praxis_linux_sandbox_exe.as_ref(),
            use_legacy_landlock,
            windows_sandbox_level,
            windows_sandbox_private_desktop,
        })
        .map(|request| ExecRequest::from_sandbox_exec_request(request, options))
        .map_err(PraxisErr::from)?;
    exec_req.windows_restricted_token_filesystem_overlay =
        resolve_windows_restricted_token_filesystem_overlay(
            exec_req.sandbox,
            &exec_req.sandbox_policy,
            &exec_req.file_system_sandbox_policy,
            exec_req.network_sandbox_policy,
            sandbox_cwd,
            exec_req.windows_sandbox_level,
        )
        .map_err(PraxisErr::UnsupportedOperation)?;
    Ok(exec_req)
}

pub(crate) async fn execute_exec_request(
    exec_request: ExecRequest,
    stdout_stream: Option<StdoutStream>,
    after_spawn: Option<SpawnObserver>,
) -> Result<ExecToolCallOutput> {
    let ExecRequest {
        command,
        cwd,
        env,
        network,
        expiration,
        capture_policy,
        sandbox,
        windows_sandbox_level,
        windows_sandbox_private_desktop,
        sandbox_policy,
        file_system_sandbox_policy,
        network_sandbox_policy,
        windows_restricted_token_filesystem_overlay,
        raw_output_spool,
        arg0,
    } = exec_request;

    let params = ExecParams {
        command,
        cwd,
        expiration,
        capture_policy,
        env,
        network: network.clone(),
        sandbox_permissions: SandboxPermissions::UseDefault,
        windows_sandbox_level,
        windows_sandbox_private_desktop,
        justification: None,
        arg0,
    };

    let start = Instant::now();
    let raw_output_result = exec(
        params,
        sandbox,
        &sandbox_policy,
        &file_system_sandbox_policy,
        windows_restricted_token_filesystem_overlay.as_ref(),
        network_sandbox_policy,
        raw_output_spool,
        stdout_stream,
        after_spawn,
    )
    .await;
    let duration = start.elapsed();
    finalize_exec_result(raw_output_result, sandbox, duration)
}

#[cfg(target_os = "windows")]
fn extract_create_process_as_user_error_code(err: &str) -> Option<String> {
    let marker = "CreateProcessAsUserW failed: ";
    let start = err.find(marker)? + marker.len();
    let tail = &err[start..];
    let digits: String = tail.chars().take_while(char::is_ascii_digit).collect();
    if digits.is_empty() {
        None
    } else {
        Some(digits)
    }
}

#[cfg(target_os = "windows")]
fn windowsapps_path_kind(path: &str) -> &'static str {
    let lower = path.to_ascii_lowercase();
    if lower.contains("\\program files\\windowsapps\\") {
        return "windowsapps_package";
    }
    if lower.contains("\\appdata\\local\\microsoft\\windowsapps\\") {
        return "windowsapps_alias";
    }
    if lower.contains("\\windowsapps\\") {
        return "windowsapps_other";
    }
    "other"
}

#[cfg(target_os = "windows")]
fn record_windows_sandbox_spawn_failure(
    command_path: Option<&str>,
    windows_sandbox_level: praxis_protocol::config_types::WindowsSandboxLevel,
    err: &str,
) {
    let Some(error_code) = extract_create_process_as_user_error_code(err) else {
        return;
    };
    let path = command_path.unwrap_or("unknown");
    let exe = Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("unknown")
        .to_ascii_lowercase();
    let path_kind = windowsapps_path_kind(path);
    let level = if matches!(
        windows_sandbox_level,
        praxis_protocol::config_types::WindowsSandboxLevel::Elevated
    ) {
        "elevated"
    } else {
        "legacy"
    };
    if let Some(metrics) = praxis_otel::metrics::global() {
        let _ = metrics.counter(
            "praxis.windows_sandbox.createprocessasuserw_failed",
            /*inc*/ 1,
            &[
                ("error_code", error_code.as_str()),
                ("path_kind", path_kind),
                ("exe", exe.as_str()),
                ("level", level),
            ],
        );
    }
}

#[cfg(target_os = "windows")]
async fn exec_windows_sandbox(
    params: ExecParams,
    sandbox_policy: &SandboxPolicy,
    windows_restricted_token_filesystem_overlay: Option<&WindowsRestrictedTokenFilesystemOverlay>,
    raw_output_spool_enabled: bool,
) -> Result<RawExecToolCallOutput> {
    use crate::config::find_praxis_home;
    use praxis_windows_sandbox::run_windows_sandbox_capture_elevated;
    use praxis_windows_sandbox::run_windows_sandbox_capture_with_extra_deny_write_paths;

    let ExecParams {
        command,
        cwd,
        mut env,
        network,
        expiration,
        capture_policy,
        windows_sandbox_level,
        windows_sandbox_private_desktop,
        ..
    } = params;
    if let Some(network) = network.as_ref() {
        network.apply_to_env(&mut env);
    }

    // TODO(iceweasel-oai): run_windows_sandbox_capture should support all
    // variants of ExecExpiration, not just timeout.
    let timeout_ms = if capture_policy.uses_expiration() {
        expiration.timeout_ms()
    } else {
        None
    };

    let policy_str = serde_json::to_string(sandbox_policy).map_err(|err| {
        PraxisErr::Io(io::Error::other(format!(
            "failed to serialize Windows sandbox policy: {err}"
        )))
    })?;
    let sandbox_cwd = cwd.clone();
    let praxis_home = find_praxis_home().map_err(|err| {
        PraxisErr::Io(io::Error::other(format!(
            "windows sandbox: failed to resolve praxis_home: {err}"
        )))
    })?;
    let command_path = command.first().cloned();
    let sandbox_level = windows_sandbox_level;
    let proxy_enforced = network.is_some();
    // Windows firewall enforcement is tied to the logon-user sandbox identities, so
    // proxy-enforced sessions must use that backend even when the configured mode is
    // the default restricted-token sandbox.
    let use_elevated = proxy_enforced || matches!(sandbox_level, WindowsSandboxLevel::Elevated);
    let additional_deny_write_paths = windows_restricted_token_filesystem_overlay
        .map(|overlay| {
            overlay
                .additional_deny_write_paths
                .iter()
                .map(AbsolutePathBuf::to_path_buf)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let spawn_res = tokio::task::spawn_blocking(move || {
        if use_elevated {
            run_windows_sandbox_capture_elevated(
                praxis_windows_sandbox::ElevatedSandboxCaptureRequest {
                    policy_json_or_preset: policy_str.as_str(),
                    sandbox_policy_cwd: &sandbox_cwd,
                    praxis_home: praxis_home.as_ref(),
                    command,
                    cwd: &cwd,
                    env_map: env,
                    timeout_ms,
                    use_private_desktop: windows_sandbox_private_desktop,
                    proxy_enforced,
                },
            )
        } else {
            run_windows_sandbox_capture_with_extra_deny_write_paths(
                policy_str.as_str(),
                &sandbox_cwd,
                praxis_home.as_ref(),
                command,
                &cwd,
                env,
                timeout_ms,
                &additional_deny_write_paths,
                windows_sandbox_private_desktop,
            )
        }
    })
    .await;

    let capture = match spawn_res {
        Ok(Ok(v)) => v,
        Ok(Err(err)) => {
            record_windows_sandbox_spawn_failure(
                command_path.as_deref(),
                sandbox_level,
                &err.to_string(),
            );
            return Err(PraxisErr::Io(io::Error::other(format!(
                "windows sandbox: {err}"
            ))));
        }
        Err(join_err) => {
            return Err(PraxisErr::Io(io::Error::other(format!(
                "windows sandbox join error: {join_err}"
            ))));
        }
    };

    let exit_status = synthetic_exit_status(capture.exit_code);
    let raw_output_spool = if raw_output_spool_enabled {
        write_capture_output_spool(&capture.stdout, &capture.stderr).await
    } else {
        None
    };
    let mut stdout_text = capture.stdout;
    if let Some(max_bytes) = capture_policy.retained_bytes_cap()
        && stdout_text.len() > max_bytes
    {
        stdout_text.truncate(max_bytes);
    }
    let mut stderr_text = capture.stderr;
    if let Some(max_bytes) = capture_policy.retained_bytes_cap()
        && stderr_text.len() > max_bytes
    {
        stderr_text.truncate(max_bytes);
    }
    let stdout = StreamOutput {
        text: stdout_text,
        truncated_after_lines: None,
    };
    let stderr = StreamOutput {
        text: stderr_text,
        truncated_after_lines: None,
    };
    let aggregated_output = aggregate_output(&stdout, &stderr, capture_policy.retained_bytes_cap());

    Ok(RawExecToolCallOutput {
        exit_status,
        stdout,
        stderr,
        aggregated_output,
        timed_out: capture.timed_out,
        raw_output_spool,
    })
}

fn finalize_exec_result(
    raw_output_result: std::result::Result<RawExecToolCallOutput, PraxisErr>,
    sandbox_type: SandboxType,
    duration: Duration,
) -> Result<ExecToolCallOutput> {
    match raw_output_result {
        Ok(raw_output) => {
            #[allow(unused_mut)]
            let mut timed_out = raw_output.timed_out;

            #[cfg(target_family = "unix")]
            {
                if let Some(signal) = raw_output.exit_status.signal() {
                    if signal == TIMEOUT_CODE {
                        timed_out = true;
                    } else {
                        return Err(PraxisErr::Sandbox(SandboxErr::Signal(signal)));
                    }
                }
            }

            let mut exit_code = raw_output.exit_status.code().unwrap_or(-1);
            if timed_out {
                exit_code = EXEC_TIMEOUT_EXIT_CODE;
            }

            let stdout = raw_output.stdout.from_utf8_lossy();
            let stderr = raw_output.stderr.from_utf8_lossy();
            let aggregated_output = raw_output.aggregated_output.from_utf8_lossy();
            let exec_output = ExecToolCallOutput {
                exit_code,
                stdout,
                stderr,
                aggregated_output,
                model_output: None,
                duration,
                timed_out,
                agent_os_artifact_id: None,
                raw_output_spool: raw_output.raw_output_spool,
            };

            if timed_out {
                return Err(PraxisErr::Sandbox(SandboxErr::Timeout {
                    output: Box::new(exec_output),
                }));
            }

            if is_likely_sandbox_denied(sandbox_type, &exec_output) {
                return Err(PraxisErr::Sandbox(SandboxErr::Denied {
                    output: Box::new(exec_output),
                    network_policy_decision: None,
                }));
            }

            Ok(exec_output)
        }
        Err(err) => {
            tracing::error!("exec error: {err}");
            Err(err)
        }
    }
}

pub(crate) mod errors {
    use super::PraxisErr;
    use praxis_sandboxing::SandboxTransformError;

    impl From<SandboxTransformError> for PraxisErr {
        fn from(err: SandboxTransformError) -> Self {
            match err {
                SandboxTransformError::MissingLinuxSandboxExecutable => {
                    PraxisErr::LandlockSandboxExecutableNotProvided
                }
                #[cfg(not(target_os = "macos"))]
                SandboxTransformError::SeatbeltUnavailable => PraxisErr::UnsupportedOperation(
                    "seatbelt sandbox is only available on macOS".to_string(),
                ),
            }
        }
    }
}

/// We don't have a fully deterministic way to tell if our command failed
/// because of the sandbox - a command in the user's zshrc file might hit an
/// error, but the command itself might fail or succeed for other reasons.
/// For now, we conservatively check for well known command failure exit codes and
/// also look for common sandbox denial keywords in the command output.
pub(crate) fn is_likely_sandbox_denied(
    sandbox_type: SandboxType,
    exec_output: &ExecToolCallOutput,
) -> bool {
    if sandbox_type == SandboxType::None || exec_output.exit_code == 0 {
        return false;
    }

    // Quick rejects: well-known non-sandbox shell exit codes
    // 2: misuse of shell builtins
    // 126: permission denied
    // 127: command not found
    const SANDBOX_DENIED_KEYWORDS: [&str; 7] = [
        "operation not permitted",
        "permission denied",
        "read-only file system",
        "seccomp",
        "sandbox",
        "landlock",
        "failed to write file",
    ];

    let has_sandbox_keyword = [
        &exec_output.stderr.text,
        &exec_output.stdout.text,
        &exec_output.aggregated_output.text,
    ]
    .into_iter()
    .any(|section| {
        let lower = section.to_lowercase();
        SANDBOX_DENIED_KEYWORDS
            .iter()
            .any(|needle| lower.contains(needle))
    });

    if has_sandbox_keyword {
        return true;
    }

    const QUICK_REJECT_EXIT_CODES: [i32; 3] = [2, 126, 127];
    if QUICK_REJECT_EXIT_CODES.contains(&exec_output.exit_code) {
        return false;
    }

    #[cfg(unix)]
    {
        const SIGSYS_CODE: i32 = libc::SIGSYS;
        if sandbox_type == SandboxType::LinuxSeccomp
            && exec_output.exit_code == EXIT_CODE_SIGNAL_BASE + SIGSYS_CODE
        {
            return true;
        }
    }

    false
}

#[allow(clippy::too_many_arguments)]
async fn exec(
    params: ExecParams,
    _sandbox: SandboxType,
    _sandbox_policy: &SandboxPolicy,
    _file_system_sandbox_policy: &FileSystemSandboxPolicy,
    _windows_restricted_token_filesystem_overlay: Option<&WindowsRestrictedTokenFilesystemOverlay>,
    network_sandbox_policy: NetworkSandboxPolicy,
    raw_output_spool: bool,
    stdout_stream: Option<StdoutStream>,
    after_spawn: Option<SpawnObserver>,
) -> Result<RawExecToolCallOutput> {
    #[cfg(target_os = "windows")]
    if _sandbox == SandboxType::WindowsRestrictedToken {
        return exec_windows_sandbox(
            params,
            _sandbox_policy,
            _windows_restricted_token_filesystem_overlay,
            raw_output_spool,
        )
        .await;
    }
    let ExecParams {
        command,
        cwd,
        mut env,
        network,
        arg0,
        expiration,
        capture_policy,
        windows_sandbox_level: _,
        ..
    } = params;
    if let Some(network) = network.as_ref() {
        network.apply_to_env(&mut env);
    }

    let (program, args) = command.split_first().ok_or_else(|| {
        PraxisErr::Io(io::Error::new(
            io::ErrorKind::InvalidInput,
            "command args are empty",
        ))
    })?;
    let arg0_ref = arg0.as_deref();
    let child = spawn_child_async(SpawnChildRequest {
        program: PathBuf::from(program),
        args: args.into(),
        arg0: arg0_ref,
        cwd,
        network_sandbox_policy,
        // The environment already has attempt-scoped proxy settings from
        // apply_to_env_for_attempt above. Passing network here would reapply
        // non-attempt proxy vars and drop attempt correlation metadata.
        network: None,
        stdio_policy: StdioPolicy::RedirectForShellTool,
        env,
    })
    .await?;
    if let Some(after_spawn) = after_spawn {
        after_spawn(child.id()).await;
    }
    consume_output(
        child,
        expiration,
        capture_policy,
        raw_output_spool,
        stdout_stream,
    )
    .await
}

#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
fn should_use_windows_restricted_token_sandbox(
    sandbox: SandboxType,
    sandbox_policy: &SandboxPolicy,
    file_system_sandbox_policy: &FileSystemSandboxPolicy,
) -> bool {
    sandbox == SandboxType::WindowsRestrictedToken
        && file_system_sandbox_policy.kind == FileSystemSandboxKind::Restricted
        && !matches!(
            sandbox_policy,
            SandboxPolicy::DangerFullAccess | SandboxPolicy::ExternalSandbox { .. }
        )
}

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn unsupported_windows_restricted_token_sandbox_reason(
    sandbox: SandboxType,
    sandbox_policy: &SandboxPolicy,
    file_system_sandbox_policy: &FileSystemSandboxPolicy,
    network_sandbox_policy: NetworkSandboxPolicy,
    sandbox_policy_cwd: &Path,
    windows_sandbox_level: WindowsSandboxLevel,
) -> Option<String> {
    resolve_windows_restricted_token_filesystem_overlay(
        sandbox,
        sandbox_policy,
        file_system_sandbox_policy,
        network_sandbox_policy,
        sandbox_policy_cwd,
        windows_sandbox_level,
    )
    .err()
}

pub(crate) fn resolve_windows_restricted_token_filesystem_overlay(
    sandbox: SandboxType,
    sandbox_policy: &SandboxPolicy,
    file_system_sandbox_policy: &FileSystemSandboxPolicy,
    network_sandbox_policy: NetworkSandboxPolicy,
    sandbox_policy_cwd: &Path,
    windows_sandbox_level: WindowsSandboxLevel,
) -> std::result::Result<Option<WindowsRestrictedTokenFilesystemOverlay>, String> {
    if sandbox != SandboxType::WindowsRestrictedToken {
        return Ok(None);
    }

    let needs_direct_runtime_enforcement = file_system_sandbox_policy
        .needs_direct_runtime_enforcement(network_sandbox_policy, sandbox_policy_cwd);

    if should_use_windows_restricted_token_sandbox(
        sandbox,
        sandbox_policy,
        file_system_sandbox_policy,
    ) && !needs_direct_runtime_enforcement
    {
        return Ok(None);
    }

    if !should_use_windows_restricted_token_sandbox(
        sandbox,
        sandbox_policy,
        file_system_sandbox_policy,
    ) {
        return Err(format!(
            "windows sandbox backend cannot enforce file_system={:?}, network={network_sandbox_policy:?}, legacy_policy={sandbox_policy:?}; refusing to run unsandboxed",
            file_system_sandbox_policy.kind,
        ));
    }

    if windows_sandbox_level != WindowsSandboxLevel::RestrictedToken {
        return Err(
            "windows elevated sandbox backend cannot enforce split filesystem permissions directly; refusing to run unsandboxed"
                .to_string(),
        );
    }

    if !file_system_sandbox_policy.has_full_disk_read_access() {
        return Err(
            "windows unelevated restricted-token sandbox cannot enforce split filesystem read restrictions directly; refusing to run unsandboxed"
                .to_string(),
        );
    }

    if !file_system_sandbox_policy
        .get_unreadable_roots_with_cwd(sandbox_policy_cwd)
        .is_empty()
    {
        return Err(
            "windows unelevated restricted-token sandbox cannot enforce unreadable split filesystem carveouts directly; refusing to run unsandboxed"
                .to_string(),
        );
    }

    let legacy_writable_roots = sandbox_policy.get_writable_roots_with_cwd(sandbox_policy_cwd);
    let split_writable_roots =
        file_system_sandbox_policy.get_writable_roots_with_cwd(sandbox_policy_cwd);
    let legacy_root_paths: BTreeSet<PathBuf> = legacy_writable_roots
        .iter()
        .map(|root| normalize_windows_overlay_path(root.root.as_path()))
        .collect::<std::result::Result<_, _>>()?;
    let split_root_paths: BTreeSet<PathBuf> = split_writable_roots
        .iter()
        .map(|root| normalize_windows_overlay_path(root.root.as_path()))
        .collect::<std::result::Result<_, _>>()?;

    if legacy_root_paths != split_root_paths {
        return Err(
            "windows unelevated restricted-token sandbox cannot enforce split writable root sets directly; refusing to run unsandboxed"
                .to_string(),
        );
    }

    for writable_root in &split_writable_roots {
        for read_only_subpath in &writable_root.read_only_subpaths {
            if split_writable_roots.iter().any(|candidate| {
                candidate.root.as_path() != writable_root.root.as_path()
                    && candidate
                        .root
                        .as_path()
                        .starts_with(read_only_subpath.as_path())
            }) {
                return Err(
                    "windows unelevated restricted-token sandbox cannot reopen writable descendants under read-only carveouts directly; refusing to run unsandboxed"
                        .to_string(),
                );
            }
        }
    }

    let mut additional_deny_write_paths = BTreeSet::new();
    for split_root in &split_writable_roots {
        let split_root_path = normalize_windows_overlay_path(split_root.root.as_path())?;
        let Some(legacy_root) = legacy_writable_roots.iter().find(|candidate| {
            normalize_windows_overlay_path(candidate.root.as_path())
                .is_ok_and(|candidate_path| candidate_path == split_root_path)
        }) else {
            return Err(
                "windows unelevated restricted-token sandbox cannot enforce split writable root sets directly; refusing to run unsandboxed"
                    .to_string(),
            );
        };

        for read_only_subpath in &split_root.read_only_subpaths {
            if !legacy_root
                .read_only_subpaths
                .iter()
                .any(|candidate| candidate == read_only_subpath)
            {
                additional_deny_write_paths
                    .insert(normalize_windows_overlay_path(read_only_subpath.as_path())?);
            }
        }
    }

    if additional_deny_write_paths.is_empty() {
        return Ok(None);
    }

    Ok(Some(WindowsRestrictedTokenFilesystemOverlay {
        additional_deny_write_paths: additional_deny_write_paths
            .into_iter()
            .map(|path| AbsolutePathBuf::from_absolute_path(path).map_err(|err| err.to_string()))
            .collect::<std::result::Result<_, _>>()?,
    }))
}

fn normalize_windows_overlay_path(path: &Path) -> std::result::Result<PathBuf, String> {
    AbsolutePathBuf::from_absolute_path(dunce::simplified(path))
        .map(AbsolutePathBuf::into_path_buf)
        .map_err(|err| err.to_string())
}

#[cfg(unix)]
fn synthetic_exit_status(code: i32) -> ExitStatus {
    use std::os::unix::process::ExitStatusExt;
    std::process::ExitStatus::from_raw(code)
}

#[cfg(windows)]
fn synthetic_exit_status(code: i32) -> ExitStatus {
    use std::os::windows::process::ExitStatusExt;
    // On Windows the raw status is a u32. Use a direct cast to avoid
    // panicking on negative i32 values produced by prior narrowing casts.
    std::process::ExitStatus::from_raw(code as u32)
}

#[cfg(test)]
#[path = "exec_tests.rs"]
mod tests;
