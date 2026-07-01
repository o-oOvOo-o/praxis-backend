use super::catalog::LocalModelEntry;
use crate::config::find_praxis_home;
use crate::config::LocalModelHostConfig;
use crate::config::LocalModelHostKind;
use crate::error::PraxisErr;
use crate::error::Result as PraxisResult;
use once_cell::sync::Lazy;
use serde_json::Value;
use sha1::Digest;
use sha1::Sha1;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::fs;
use std::fs::OpenOptions;
use std::io::Read;
use std::io::Seek;
use std::io::SeekFrom;
use std::net::TcpListener;
use std::path::Path;
use std::path::PathBuf;
use std::process::Child;
use std::process::Command;
use std::process::Stdio;
use std::sync::Mutex;
use std::sync::Once;
use std::time::Duration;
use std::time::Instant;
use tokio::time::sleep;
use tracing::info;
use tracing::warn;
use url::Url;

const DEFAULT_LLAMA_CPP_SERVER: &str = "F:/tools/llama.cpp/build/bin/Release/llama-server.exe";
const DEFAULT_LOCAL_GPU_CONTEXT_SIZE: usize = 32_768;
const DEFAULT_LOCAL_GPU_LAYERS: usize = 999;
const DEFAULT_LOCAL_GPU_STARTUP_TIMEOUT_MS: u64 = 600_000;
const DEFAULT_LOCAL_GPU_STREAM_IDLE_TIMEOUT_MS: u64 = 600_000;
const DEFAULT_LOCAL_GPU_MAX_TOKENS: i64 = 1024;
const DEFAULT_LOCAL_GPU_HEALTH_PATH: &str = "health";
const PRAXIS_LLAMA_CPP_SERVER_ENV: &str = "PRAXIS_LLAMA_CPP_SERVER";
const PRAXIS_LOCAL_LLM_CONTEXT_ENV: &str = "PRAXIS_LOCAL_LLM_CONTEXT";
const PRAXIS_LOCAL_LLM_GPU_LAYERS_ENV: &str = "PRAXIS_LOCAL_LLM_GPU_LAYERS";
const PRAXIS_LOCAL_LLM_MAX_TOKENS_ENV: &str = "PRAXIS_LOCAL_LLM_MAX_TOKENS";
const PRAXIS_LOCAL_LLM_STARTUP_TIMEOUT_ENV: &str = "PRAXIS_LOCAL_LLM_STARTUP_TIMEOUT_MS";

static LLAMA_RUNTIME: Lazy<Mutex<Option<ManagedLlamaRuntime>>> = Lazy::new(|| Mutex::new(None));
static LLAMA_RUNTIME_SHUTDOWN_HOOK: Once = Once::new();

struct ManagedLlamaRuntime {
    key: String,
    api_base_url: String,
    child: Child,
}

impl ManagedLlamaRuntime {
    fn is_running(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }
}

impl Drop for ManagedLlamaRuntime {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

struct LlamaServerEndpoint {
    host: String,
    port: u16,
    root_url: String,
    api_base_url: String,
}

struct LlamaServerLogPaths {
    stdout: Option<PathBuf>,
    stderr: Option<PathBuf>,
}

enum ExistingLlamaServerProbe {
    Unreachable,
    MatchingModel,
    DifferentModel,
}

pub(super) async fn ensure_managed_llama_gpu_server(
    entry: &LocalModelEntry,
    host: Option<&LocalModelHostConfig>,
) -> PraxisResult<String> {
    if matches!(
        host.map(|host| host.kind),
        Some(LocalModelHostKind::NativeEngine)
    ) {
        return Err(PraxisErr::UnsupportedOperation(
            "local LLM native_engine CPU inference has been removed; use managed_server with a GPU llama.cpp build".to_string(),
        ));
    }

    let command = resolve_llama_server_command(host)?;
    validate_llama_server_gpu_backend(&command)?;
    register_llama_runtime_shutdown_hook();
    let key = llama_runtime_key(entry, host, &command);
    let endpoint = llama_server_endpoint(host)?;
    {
        let mut cache = LLAMA_RUNTIME
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(runtime) = cache.as_mut() {
            if runtime.key == key && runtime.is_running() {
                return Ok(runtime.api_base_url.clone());
            }
            let stale = cache.take();
            drop(stale);
        }
    }

    if host.and_then(|host| host.base_url.as_ref()).is_some() {
        match probe_existing_llama_server(&endpoint, entry, host).await? {
            ExistingLlamaServerProbe::MatchingModel => {
                info!(
                    "reusing existing local GPU llama.cpp server model_path={} base_url={}",
                    entry.model_path.display(),
                    endpoint.api_base_url
                );
                return Ok(endpoint.api_base_url);
            }
            ExistingLlamaServerProbe::DifferentModel => {
                return Err(PraxisErr::UnsupportedOperation(format!(
                    "local GPU llama.cpp endpoint {} is already running but does not expose {}; stop that server or configure another local_model_hosts.*.base_url",
                    endpoint.api_base_url,
                    entry.model_path.display()
                )));
            }
            ExistingLlamaServerProbe::Unreachable => {}
        }
    }

    let args = llama_server_args(entry, host, &endpoint);
    validate_llama_server_gpu_args(&args)?;
    let log_paths = prepare_llama_server_logs(entry, &endpoint);
    let mut command_builder = Command::new(&command);
    command_builder
        .args(&args)
        .stdin(Stdio::null());
    attach_llama_server_logs(&mut command_builder, &log_paths);
    apply_host_env(&mut command_builder, host);

    info!(
        "starting local GPU llama.cpp server command={} model_path={} base_url={} stdout_log={} stderr_log={}",
        command.display(),
        entry.model_path.display(),
        endpoint.api_base_url,
        log_paths
            .stdout
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "<discarded>".to_string()),
        log_paths
            .stderr
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "<discarded>".to_string())
    );
    let mut child = command_builder.spawn().map_err(|err| {
        PraxisErr::UnsupportedOperation(format!(
            "failed to start local GPU llama.cpp server `{}`: {err}",
            command.display()
        ))
    })?;
    wait_for_llama_server_ready(&endpoint.root_url, host, &mut child, log_paths.stderr.as_deref())
        .await?;

    let mut cache = LLAMA_RUNTIME
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(runtime) = cache.as_mut()
        && runtime.key == key
        && runtime.is_running()
    {
        let api_base_url = runtime.api_base_url.clone();
        let _ = child.kill();
        let _ = child.wait();
        return Ok(api_base_url);
    }
    *cache = Some(ManagedLlamaRuntime {
        key,
        api_base_url: endpoint.api_base_url.clone(),
        child,
    });
    Ok(endpoint.api_base_url)
}

fn register_llama_runtime_shutdown_hook() {
    LLAMA_RUNTIME_SHUTDOWN_HOOK.call_once(|| unsafe {
        libc::atexit(shutdown_llama_runtime);
    });
}

extern "C" fn shutdown_llama_runtime() {
    if let Ok(mut runtime) = LLAMA_RUNTIME.lock() {
        let stale = runtime.take();
        drop(stale);
    }
}

fn resolve_llama_server_command(host: Option<&LocalModelHostConfig>) -> PraxisResult<PathBuf> {
    if let Some(command) = host
        .and_then(|host| host.command.as_deref())
        .filter(|command| !command.trim().is_empty())
    {
        return resolve_command_path(command);
    }
    if let Ok(command) = std::env::var(PRAXIS_LLAMA_CPP_SERVER_ENV)
        && !command.trim().is_empty()
    {
        return resolve_command_path(&command);
    }
    let default = PathBuf::from(DEFAULT_LLAMA_CPP_SERVER);
    if default.is_file() {
        return Ok(default);
    }
    which::which("llama-server").map_err(|err| {
        PraxisErr::UnsupportedOperation(format!(
            "no GPU llama.cpp server found; set {PRAXIS_LLAMA_CPP_SERVER_ENV} or local_model_hosts.*.command: {err}"
        ))
    })
}

fn resolve_command_path(command: &str) -> PraxisResult<PathBuf> {
    let path = PathBuf::from(command);
    if path.is_file() {
        return Ok(path);
    }
    which::which(command).map_err(|err| {
        PraxisErr::UnsupportedOperation(format!(
            "local GPU server command `{command}` not found: {err}"
        ))
    })
}

fn validate_llama_server_gpu_backend(command: &Path) -> PraxisResult<()> {
    let Some(dir) = command.parent() else {
        return Ok(());
    };
    const GPU_BACKEND_DLLS: [&str; 6] = [
        "ggml-cuda.dll",
        "ggml-vulkan.dll",
        "ggml-hip.dll",
        "ggml-kompute.dll",
        "ggml-sycl.dll",
        "ggml-opencl.dll",
    ];
    if GPU_BACKEND_DLLS.iter().any(|dll| dir.join(dll).is_file()) {
        return Ok(());
    }
    Err(PraxisErr::UnsupportedOperation(format!(
        "refusing to run CPU-only llama.cpp build at `{}`; rebuild llama.cpp with CUDA or Vulkan so one of {} exists beside llama-server",
        command.display(),
        GPU_BACKEND_DLLS.join(", ")
    )))
}

fn validate_llama_server_gpu_args(args: &[String]) -> PraxisResult<()> {
    let Some(value) = find_arg_value(args, &["-ngl", "--gpu-layers", "--n-gpu-layers"]) else {
        return Ok(());
    };
    let Some(gpu_layers) = value.parse::<isize>().ok() else {
        return Ok(());
    };
    if gpu_layers <= 0 {
        return Err(PraxisErr::UnsupportedOperation(format!(
            "refusing to run local LLM with CPU-only gpu layer setting `{value}`; set -ngl/--gpu-layers above zero"
        )));
    }
    Ok(())
}

fn llama_server_endpoint(host: Option<&LocalModelHostConfig>) -> PraxisResult<LlamaServerEndpoint> {
    if let Some(base_url) = host.and_then(|host| host.base_url.as_deref()) {
        return endpoint_from_base_url(base_url);
    }
    let port = reserve_local_port()?;
    Ok(LlamaServerEndpoint {
        host: "127.0.0.1".to_string(),
        port,
        root_url: format!("http://127.0.0.1:{port}"),
        api_base_url: format!("http://127.0.0.1:{port}/v1"),
    })
}

fn endpoint_from_base_url(base_url: &str) -> PraxisResult<LlamaServerEndpoint> {
    let parsed = Url::parse(base_url).map_err(|err| {
        PraxisErr::InvalidRequest(format!(
            "invalid local model host base_url `{base_url}`: {err}"
        ))
    })?;
    let host = parsed.host_str().ok_or_else(|| {
        PraxisErr::InvalidRequest(format!(
            "local model host base_url `{base_url}` has no host"
        ))
    })?;
    let port = parsed.port().ok_or_else(|| {
        PraxisErr::InvalidRequest(format!(
            "managed local model host base_url `{base_url}` must include an explicit port"
        ))
    })?;
    let root_url = root_url_from_base_url(base_url);
    Ok(LlamaServerEndpoint {
        host: host.to_string(),
        port,
        api_base_url: ensure_v1_base_url(base_url),
        root_url,
    })
}

fn reserve_local_port() -> PraxisResult<u16> {
    let listener = TcpListener::bind(("127.0.0.1", 0)).map_err(|err| {
        PraxisErr::UnsupportedOperation(format!("failed to reserve local LLM server port: {err}"))
    })?;
    listener
        .local_addr()
        .map(|addr| addr.port())
        .map_err(|err| {
            PraxisErr::UnsupportedOperation(format!("failed to read reserved port: {err}"))
        })
}

fn prepare_llama_server_logs(
    entry: &LocalModelEntry,
    endpoint: &LlamaServerEndpoint,
) -> LlamaServerLogPaths {
    let Some(log_dir) = find_praxis_home()
        .ok()
        .map(|home| home.join("log"))
        .filter(|path| fs::create_dir_all(path).is_ok())
    else {
        return LlamaServerLogPaths {
            stdout: None,
            stderr: None,
        };
    };
    let hash = short_llama_runtime_hash(entry, endpoint);
    LlamaServerLogPaths {
        stdout: Some(log_dir.join(format!(
            "local-llm-{}-{hash}.stdout.log",
            endpoint.port
        ))),
        stderr: Some(log_dir.join(format!(
            "local-llm-{}-{hash}.stderr.log",
            endpoint.port
        ))),
    }
}

fn attach_llama_server_logs(command: &mut Command, paths: &LlamaServerLogPaths) {
    if let Some(path) = paths.stdout.as_deref()
        && let Ok(file) = truncate_log_file(path)
    {
        command.stdout(file);
    } else {
        command.stdout(Stdio::null());
    }
    if let Some(path) = paths.stderr.as_deref()
        && let Ok(file) = truncate_log_file(path)
    {
        command.stderr(file);
    } else {
        command.stderr(Stdio::null());
    }
}

fn truncate_log_file(path: &Path) -> std::io::Result<std::fs::File> {
    OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(path)
}

fn short_llama_runtime_hash(entry: &LocalModelEntry, endpoint: &LlamaServerEndpoint) -> String {
    let mut hasher = Sha1::new();
    hasher.update(entry.model_path.to_string_lossy().as_bytes());
    hasher.update(endpoint.api_base_url.as_bytes());
    let digest = hasher.finalize();
    digest[..4]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn llama_server_args(
    entry: &LocalModelEntry,
    host: Option<&LocalModelHostConfig>,
    endpoint: &LlamaServerEndpoint,
) -> Vec<String> {
    let mut args = host
        .filter(|host| !host.args.is_empty())
        .map(|host| {
            host.args
                .iter()
                .map(|arg| expand_llama_arg(arg, entry, host, endpoint))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    push_arg_pair_if_missing(
        &mut args,
        &["-m", "--model"],
        "-m",
        entry.model_path.to_string_lossy().as_ref(),
    );
    push_arg_pair_if_missing(&mut args, &["--host"], "--host", &endpoint.host);
    push_arg_pair_if_missing(&mut args, &["--port"], "--port", &endpoint.port.to_string());
    push_arg_pair_if_missing(
        &mut args,
        &["-ngl", "--gpu-layers", "--n-gpu-layers"],
        "-ngl",
        &local_gpu_layers(host).to_string(),
    );
    push_arg_pair_if_missing(
        &mut args,
        &["-c", "--ctx-size"],
        "-c",
        &local_context_size(host).to_string(),
    );
    args
}

fn expand_llama_arg(
    arg: &str,
    entry: &LocalModelEntry,
    host: &LocalModelHostConfig,
    endpoint: &LlamaServerEndpoint,
) -> String {
    arg.replace("{model_path}", entry.model_path.to_string_lossy().as_ref())
        .replace("{host}", &endpoint.host)
        .replace("{port}", &endpoint.port.to_string())
        .replace("{ctx}", &local_context_size(Some(host)).to_string())
        .replace("{gpu_layers}", &local_gpu_layers(Some(host)).to_string())
}

fn push_arg_pair_if_missing(args: &mut Vec<String>, names: &[&str], name: &str, value: &str) {
    if args_have_any_flag(args, names) {
        return;
    }
    args.push(name.to_string());
    args.push(value.to_string());
}

fn args_have_any_flag(args: &[String], names: &[&str]) -> bool {
    args.iter().any(|arg| {
        names.iter().any(|name| {
            arg == name
                || arg
                    .strip_prefix(name)
                    .is_some_and(|tail| tail.starts_with('='))
        })
    })
}

fn find_arg_value<'a>(args: &'a [String], names: &[&str]) -> Option<&'a str> {
    for (index, arg) in args.iter().enumerate() {
        for name in names {
            if arg == name {
                return args.get(index + 1).map(String::as_str);
            }
            if let Some(value) = arg.strip_prefix(&format!("{name}=")) {
                return Some(value);
            }
        }
    }
    None
}

fn apply_host_env(command: &mut Command, host: Option<&LocalModelHostConfig>) {
    let Some(host) = host else {
        return;
    };
    for (key, value) in &host.env {
        command.env(key, value);
    }
}

async fn wait_for_llama_server_ready(
    root_url: &str,
    host: Option<&LocalModelHostConfig>,
    child: &mut Child,
    stderr_log_path: Option<&Path>,
) -> PraxisResult<()> {
    let client = local_llama_health_client()?;
    let health_url = llama_health_url(root_url, host);
    let timeout = Duration::from_millis(startup_timeout_ms(host));
    let started = Instant::now();
    loop {
        if let Some(status) = child.try_wait().map_err(|err| {
            PraxisErr::UnsupportedOperation(format!(
                "failed to inspect local GPU llama.cpp server process: {err}"
            ))
        })? {
            let tail = log_tail_suffix(stderr_log_path);
            return Err(PraxisErr::UnsupportedOperation(format!(
                "local GPU llama.cpp server exited before becoming ready: {status}{tail}"
            )));
        }
        if started.elapsed() > timeout {
            let tail = log_tail_suffix(stderr_log_path);
            return Err(PraxisErr::UnsupportedOperation(format!(
                "local GPU llama.cpp server did not become ready within {} ms{tail}",
                timeout.as_millis()
            )));
        }
        match client.get(&health_url).send().await {
            Ok(response) if response.status().is_success() => return Ok(()),
            Ok(response) => {
                warn!(
                    "local GPU llama.cpp health check not ready status={} url={}",
                    response.status(),
                    health_url
                );
            }
            Err(_) => {}
        }
        sleep(Duration::from_millis(500)).await;
    }
}

async fn probe_existing_llama_server(
    endpoint: &LlamaServerEndpoint,
    entry: &LocalModelEntry,
    host: Option<&LocalModelHostConfig>,
) -> PraxisResult<ExistingLlamaServerProbe> {
    let client = local_llama_health_client()?;
    let health_url = llama_health_url(&endpoint.root_url, host);
    let Ok(response) = client.get(&health_url).send().await else {
        return Ok(ExistingLlamaServerProbe::Unreachable);
    };
    if !response.status().is_success() {
        return Ok(ExistingLlamaServerProbe::Unreachable);
    }

    let models_url = format!("{}/models", endpoint.api_base_url.trim_end_matches('/'));
    let Ok(response) = client.get(&models_url).send().await else {
        return Ok(ExistingLlamaServerProbe::DifferentModel);
    };
    if !response.status().is_success() {
        return Ok(ExistingLlamaServerProbe::DifferentModel);
    }
    let response_json = response.text().await.map_err(|err| {
        PraxisErr::UnsupportedOperation(format!(
            "failed to read local GPU llama.cpp models response: {err}"
        ))
    })?;
    let value = serde_json::from_str::<Value>(&response_json).map_err(|err| {
        PraxisErr::UnsupportedOperation(format!(
            "failed to parse local GPU llama.cpp models response: {err}"
        ))
    })?;
    Ok(
        llama_models_response_contains_entry(&value, entry, local_context_size(host))
            .then_some(ExistingLlamaServerProbe::MatchingModel)
            .unwrap_or(ExistingLlamaServerProbe::DifferentModel),
    )
}

fn local_llama_health_client() -> PraxisResult<reqwest::Client> {
    reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(5))
        .build()
        .map_err(|err| {
            PraxisErr::UnsupportedOperation(format!(
                "failed to build local LLM health client: {err}"
            ))
        })
}

fn llama_health_url(root_url: &str, host: Option<&LocalModelHostConfig>) -> String {
    format!(
        "{}/{}",
        root_url.trim_end_matches('/'),
        health_path(host).trim_start_matches('/')
    )
}

fn log_tail_suffix(path: Option<&Path>) -> String {
    read_log_tail(path)
        .map(|tail| format!("; stderr tail:\n{tail}"))
        .unwrap_or_default()
}

fn read_log_tail(path: Option<&Path>) -> Option<String> {
    let path = path?;
    let mut file = fs::File::open(path).ok()?;
    let len = file.metadata().ok()?.len();
    const MAX_TAIL_BYTES: u64 = 12 * 1024;
    if len > MAX_TAIL_BYTES {
        file.seek(SeekFrom::Start(len - MAX_TAIL_BYTES)).ok()?;
    }
    let mut text = String::new();
    file.read_to_string(&mut text).ok()?;
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_string())
}

fn llama_models_response_contains_entry(
    value: &Value,
    entry: &LocalModelEntry,
    min_context_size: usize,
) -> bool {
    let targets = llama_model_ref_targets(entry);
    ["data", "models"].iter().any(|key| {
        value
            .get(key)
            .and_then(Value::as_array)
            .is_some_and(|models| {
                models
                    .iter()
                    .any(|model| llama_model_value_matches(model, &targets, min_context_size))
            })
    })
}

fn llama_model_value_matches(
    value: &Value,
    targets: &BTreeSet<String>,
    min_context_size: usize,
) -> bool {
    let model_matches = ["id", "name", "model"].iter().any(|key| {
        value
            .get(key)
            .and_then(Value::as_str)
            .is_some_and(|candidate| normalized_llama_model_ref_matches(candidate, targets))
    }) || value.get("aliases").and_then(Value::as_array).is_some_and(
        |aliases| {
            aliases.iter().any(|alias| {
                alias
                    .as_str()
                    .is_some_and(|candidate| normalized_llama_model_ref_matches(candidate, targets))
            })
        },
    );
    model_matches && llama_model_context_is_usable(value, min_context_size)
}

fn llama_model_context_is_usable(value: &Value, min_context_size: usize) -> bool {
    value
        .get("meta")
        .and_then(|meta| meta.get("n_ctx"))
        .and_then(Value::as_u64)
        .map(|n_ctx| n_ctx as usize >= min_context_size)
        .unwrap_or(true)
}

fn normalize_llama_model_ref(value: &str) -> String {
    value.trim().replace('\\', "/").to_ascii_lowercase()
}

fn normalized_llama_model_ref_matches(candidate: &str, targets: &BTreeSet<String>) -> bool {
    let candidate = normalize_llama_model_ref(candidate);
    targets.contains(&candidate)
}

fn llama_model_ref_targets(entry: &LocalModelEntry) -> BTreeSet<String> {
    let mut targets = BTreeSet::new();
    push_llama_model_ref_target(&mut targets, entry.model_path.to_string_lossy().as_ref());
    push_llama_model_ref_target(&mut targets, &entry.model_id);
    push_llama_model_ref_target(&mut targets, &entry.display_name);
    for alias in &entry.aliases {
        push_llama_model_ref_target(&mut targets, alias);
    }
    if let Some(file_name) = entry
        .model_path
        .file_name()
        .and_then(|value| value.to_str())
    {
        push_llama_model_ref_target(&mut targets, file_name);
    }
    if let Some(stem) = entry
        .model_path
        .file_stem()
        .and_then(|value| value.to_str())
    {
        push_llama_model_ref_target(&mut targets, stem);
    }
    targets
}

fn push_llama_model_ref_target(targets: &mut BTreeSet<String>, value: &str) {
    let normalized = normalize_llama_model_ref(value);
    if !normalized.is_empty() {
        targets.insert(normalized);
    }
}

fn health_path(host: Option<&LocalModelHostConfig>) -> String {
    host.and_then(|host| host.health_path.clone())
        .filter(|path| !path.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_LOCAL_GPU_HEALTH_PATH.to_string())
}

pub(super) fn ensure_v1_base_url(base_url: &str) -> String {
    let base = base_url.trim_end_matches('/');
    if base.ends_with("/v1") {
        base.to_string()
    } else {
        format!("{base}/v1")
    }
}

fn root_url_from_base_url(base_url: &str) -> String {
    let base = base_url.trim_end_matches('/');
    base.strip_suffix("/v1").unwrap_or(base).to_string()
}

fn llama_runtime_key(
    entry: &LocalModelEntry,
    host: Option<&LocalModelHostConfig>,
    command: &Path,
) -> String {
    let mut env = BTreeMap::<String, String>::new();
    if let Some(host) = host {
        env = host.env.clone();
    }
    format!(
        "{}|{}|{:?}|{:?}|{:?}|{}|{}",
        command.display(),
        entry.model_path.display(),
        host.and_then(|host| host.base_url.as_ref()),
        host.map(|host| &host.args),
        env,
        local_context_size(host),
        local_gpu_layers(host)
    )
}

pub(super) fn local_stream_idle_timeout_ms(host: Option<&LocalModelHostConfig>) -> u64 {
    host.and_then(|host| host.idle_timeout_ms)
        .filter(|value| *value > 0)
        .unwrap_or(DEFAULT_LOCAL_GPU_STREAM_IDLE_TIMEOUT_MS)
}

pub(super) fn local_max_tokens(host: Option<&LocalModelHostConfig>) -> i64 {
    metadata_i64(host, "max_tokens")
        .or_else(|| env_i64(PRAXIS_LOCAL_LLM_MAX_TOKENS_ENV))
        .unwrap_or(DEFAULT_LOCAL_GPU_MAX_TOKENS)
}

fn local_context_size(host: Option<&LocalModelHostConfig>) -> usize {
    metadata_usize(host, "context_size")
        .or_else(|| metadata_usize(host, "ctx_size"))
        .or_else(|| env_usize(PRAXIS_LOCAL_LLM_CONTEXT_ENV))
        .unwrap_or(DEFAULT_LOCAL_GPU_CONTEXT_SIZE)
}

fn local_gpu_layers(host: Option<&LocalModelHostConfig>) -> usize {
    metadata_usize(host, "gpu_layers")
        .or_else(|| env_usize(PRAXIS_LOCAL_LLM_GPU_LAYERS_ENV))
        .unwrap_or(DEFAULT_LOCAL_GPU_LAYERS)
}

fn startup_timeout_ms(host: Option<&LocalModelHostConfig>) -> u64 {
    metadata_u64(host, "startup_timeout_ms")
        .or_else(|| env_u64(PRAXIS_LOCAL_LLM_STARTUP_TIMEOUT_ENV))
        .unwrap_or(DEFAULT_LOCAL_GPU_STARTUP_TIMEOUT_MS)
}

fn metadata_u64(host: Option<&LocalModelHostConfig>, key: &str) -> Option<u64> {
    host.and_then(|host| host.metadata.get(key))
        .and_then(Value::as_u64)
}

fn metadata_i64(host: Option<&LocalModelHostConfig>, key: &str) -> Option<i64> {
    host.and_then(|host| host.metadata.get(key))
        .and_then(|value| {
            value
                .as_i64()
                .or_else(|| value.as_u64().and_then(|value| i64::try_from(value).ok()))
        })
        .filter(|value| *value > 0)
}

fn metadata_usize(host: Option<&LocalModelHostConfig>, key: &str) -> Option<usize> {
    host.and_then(|host| host.metadata.get(key))
        .and_then(|value| {
            value
                .as_u64()
                .and_then(|value| usize::try_from(value).ok())
                .or_else(|| {
                    value
                        .as_i64()
                        .and_then(|value| usize::try_from(value).ok())
                })
        })
        .filter(|value| *value > 0)
}

fn env_usize(name: &str) -> Option<usize> {
    std::env::var(name)
        .ok()
        .and_then(|value| value.trim().parse::<usize>().ok())
        .filter(|value| *value > 0)
}

fn env_u64(name: &str) -> Option<u64> {
    std::env::var(name)
        .ok()
        .and_then(|value| value.trim().parse::<u64>().ok())
        .filter(|value| *value > 0)
}

fn env_i64(name: &str) -> Option<i64> {
    std::env::var(name)
        .ok()
        .and_then(|value| value.trim().parse::<i64>().ok())
        .filter(|value| *value > 0)
}
