use crate::config::Config;
use crate::config::LocalModelHostConfig;
use crate::config::LocalModelHostKind;
use crate::config::LocalModelsConfig;
use crate::model_provider_info::ModelProviderInfo;
use crate::model_provider_info::NATIVE_LOCAL_PROVIDER_ID;
use crate::model_provider_info::create_native_local_provider;
use crate::model_provider_info::is_native_local_provider;
use candle_core::quantized::gguf_file::{Content, Value as GgufValue};
use once_cell::sync::Lazy;
use praxis_protocol::config_types::ReasoningSummary;
use praxis_protocol::openai_models::ConfigShellToolType;
use praxis_protocol::openai_models::InputModality;
use praxis_protocol::openai_models::ModelInfo;
use praxis_protocol::openai_models::ModelPreset;
use praxis_protocol::openai_models::ModelVisibility;
use praxis_protocol::openai_models::ReasoningEffort;
use praxis_protocol::openai_models::ReasoningEffortPreset;
use praxis_protocol::openai_models::TruncationPolicyConfig;
use praxis_protocol::openai_models::WebSearchToolType;
use praxis_utils_absolute_path::AbsolutePathBuf;
use sha1::Digest;
use sha1::Sha1;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;
use std::time::Instant;
use walkdir::WalkDir;

const TOKENIZER_FILE_NAME: &str = "tokenizer.json";
const DEFAULT_LOCAL_MODEL_CONTEXT_WINDOW: i64 = 32_768;
const NATIVE_LOCAL_BASE_INSTRUCTIONS: &str = r#"You are Praxis running on a local GPU model.

Work as a pragmatic coding agent:
- Follow the user's request and any developer, workspace, skill, or plugin instructions that are supplied in the turn.
- Inspect files with tools before making claims about code; never invent command output, file contents, or test results.
- Use shell/tools for real work, and use apply_patch for manual code edits.
- Preserve user changes and avoid destructive commands unless the user explicitly asks.
- Keep answers direct and concise. Do not expose hidden reasoning, thinking tags, or analysis text.
- If the local model is too small or slow for the task, say that plainly and recommend a stronger model or a narrower task."#;
const LOCAL_MODEL_DISCOVERY_CACHE_TTL_MS: u64 = 5_000;

static LOCAL_MODEL_DISCOVERY_CACHE: Lazy<Mutex<Option<LocalModelDiscoveryCache>>> =
    Lazy::new(|| Mutex::new(None));

#[derive(Debug, Clone)]
struct LocalModelDiscoveryCache {
    key: String,
    captured_at: Instant,
    entries: Vec<LocalModelEntry>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct NativeLocalModelConfig {
    pub local_models: LocalModelsConfig,
    pub local_model_hosts: BTreeMap<String, LocalModelHostConfig>,
}

impl NativeLocalModelConfig {
    pub(crate) fn from_config(config: &Config) -> Self {
        Self {
            local_models: config.local_models.clone(),
            local_model_hosts: config.local_model_hosts.clone(),
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct LocalModelPreset {
    pub provider_id: String,
    pub provider: ModelProviderInfo,
    pub preset: ModelPreset,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LocalModelFormat {
    Gguf,
    SafeTensors,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LocalModelArchitecture {
    Gemma,
    Llama,
    Mistral,
    Qwen2,
    Qwen3,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LocalModelWire {
    LlamaCppGpu,
    ExternalOpenAiCompat,
    Unsupported,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LocalModelEntry {
    pub model_id: String,
    pub display_name: String,
    pub aliases: Vec<String>,
    pub model_path: AbsolutePathBuf,
    pub tokenizer_path: Option<AbsolutePathBuf>,
    pub format: LocalModelFormat,
    pub architecture: LocalModelArchitecture,
    pub size_bytes: Option<u64>,
    pub wire: LocalModelWire,
    pub host_id: Option<String>,
    pub runtime_supported: bool,
}

pub(crate) fn config_uses_native_local_provider(config: &Config) -> bool {
    is_native_local_provider(config.model_provider_id.as_str(), &config.model_provider)
}

pub(crate) fn local_model_presets_for_config(config: &Config) -> Vec<LocalModelPreset> {
    let provider = native_local_provider(config);
    let entries = discover_local_models(config)
        .into_iter()
        .filter(|entry| entry.runtime_supported)
        .collect::<Vec<_>>();
    let default_index = entries
        .iter()
        .position(|entry| {
            config
                .model
                .as_deref()
                .is_some_and(|model| entry_matches_model(entry, model))
        })
        .unwrap_or(0);
    entries
        .into_iter()
        .enumerate()
        .map(|(index, entry)| {
            let host = host_for_entry(config, &entry);
            let mut preset =
                ModelPreset::from(model_info_from_entry(&entry, &entry.model_id, host));
            preset.is_default = index == default_index;
            LocalModelPreset {
                provider_id: NATIVE_LOCAL_PROVIDER_ID.to_string(),
                provider: provider.clone(),
                preset,
            }
        })
        .collect()
}

pub(crate) fn local_model_info_for_config(config: &Config, model: &str) -> Option<ModelInfo> {
    let entry = resolve_local_model(config, model)?;
    let host = host_for_entry(config, &entry);
    Some(model_info_from_entry(&entry, model, host))
}

pub(crate) fn resolve_local_model(config: &Config, model: &str) -> Option<LocalModelEntry> {
    discover_local_models_from_runtime_config(&NativeLocalModelConfig::from_config(config))
        .into_iter()
        .find(|entry| entry_matches_model(entry, model))
}

pub(crate) fn discover_local_models(config: &Config) -> Vec<LocalModelEntry> {
    discover_local_models_from_runtime_config(&NativeLocalModelConfig::from_config(config))
}

pub(crate) fn discover_local_models_from_runtime_config(
    config: &NativeLocalModelConfig,
) -> Vec<LocalModelEntry> {
    let cache_key = local_model_discovery_cache_key(config);
    {
        let cache = LOCAL_MODEL_DISCOVERY_CACHE
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(cache) = cache.as_ref()
            && cache.key == cache_key
            && cache.captured_at.elapsed()
                <= Duration::from_millis(LOCAL_MODEL_DISCOVERY_CACHE_TTL_MS)
        {
            return cache.entries.clone();
        }
    }

    let entries = discover_local_models_from_runtime_config_uncached(config);
    let mut cache = LOCAL_MODEL_DISCOVERY_CACHE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    *cache = Some(LocalModelDiscoveryCache {
        key: cache_key,
        captured_at: Instant::now(),
        entries: entries.clone(),
    });
    entries
}

fn discover_local_models_from_runtime_config_uncached(
    config: &NativeLocalModelConfig,
) -> Vec<LocalModelEntry> {
    let mut entries = BTreeMap::<PathBuf, LocalModelEntry>::new();
    let scan_max_depth = config.local_models.scan_max_depth.max(1);

    for root in &config.local_models.paths {
        for entry in discover_model_dir(root.as_path(), scan_max_depth) {
            entries.entry(model_path_key(&entry)).or_insert(entry);
        }
    }

    for entry in explicit_local_host_models(config) {
        entries.insert(model_path_key(&entry), entry);
    }

    let mut entries = entries.into_values().collect::<Vec<_>>();
    entries.sort_by(|left, right| {
        left.display_name
            .cmp(&right.display_name)
            .then_with(|| left.model_id.cmp(&right.model_id))
    });
    entries
}

fn local_model_discovery_cache_key(config: &NativeLocalModelConfig) -> String {
    let mut hasher = Sha1::new();
    hasher.update(config.local_models.scan_max_depth.to_le_bytes());
    for path in &config.local_models.paths {
        hasher.update(path.to_string_lossy().as_bytes());
        hasher.update([0]);
    }
    for (host_id, host) in &config.local_model_hosts {
        hasher.update(host_id.as_bytes());
        hasher.update([0]);
        let host_json = serde_json::to_string(host).unwrap_or_else(|_| format!("{host:?}"));
        hasher.update(host_json.as_bytes());
        hasher.update([0]);
    }
    let digest = hasher.finalize();
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn native_local_provider(config: &Config) -> ModelProviderInfo {
    config
        .model_providers
        .get(NATIVE_LOCAL_PROVIDER_ID)
        .cloned()
        .unwrap_or_else(create_native_local_provider)
}

fn model_info_from_entry(
    entry: &LocalModelEntry,
    slug: &str,
    host: Option<&LocalModelHostConfig>,
) -> ModelInfo {
    let description = match entry.format {
        LocalModelFormat::Gguf if entry.runtime_supported => format!(
            "GPU-backed local GGUF model served from {}.",
            entry.model_path.display()
        ),
        LocalModelFormat::Gguf => format!(
            "Local GGUF model at {}. Cataloged but not GPU-runnable with the current host config.",
            entry.model_path.display()
        ),
        LocalModelFormat::SafeTensors => format!(
            "Local SafeTensors model at {}. Cataloged but not native-runnable yet.",
            entry.model_path.display()
        ),
    };
    let context_window = local_model_context_window(host);
    let compact_limit = (context_window * 8) / 10;

    ModelInfo {
        slug: slug.to_string(),
        display_name: entry.display_name.clone(),
        description: Some(description),
        default_reasoning_level: Some(ReasoningEffort::None),
        supported_reasoning_levels: vec![ReasoningEffortPreset {
            effort: ReasoningEffort::None,
            display_name: None,
            description: "Local GPU inference does not expose provider reasoning controls."
                .to_string(),
        }],
        shell_type: ConfigShellToolType::Default,
        visibility: ModelVisibility::List,
        supported_in_api: true,
        priority: 90,
        availability_nux: None,
        upgrade: None,
        base_instructions: native_local_base_instructions(),
        model_messages: None,
        supports_reasoning_summaries: false,
        default_reasoning_summary: ReasoningSummary::None,
        support_verbosity: false,
        default_verbosity: None,
        apply_patch_tool_type: None,
        web_search_tool_type: WebSearchToolType::Text,
        truncation_policy: TruncationPolicyConfig::bytes(10_000),
        supports_parallel_tool_calls: false,
        supports_image_detail_original: false,
        context_window: Some(context_window),
        auto_compact_token_limit: Some(compact_limit),
        effective_context_window_percent: 85,
        experimental_supported_tools: Vec::new(),
        input_modalities: vec![InputModality::Text],
        used_fallback_model_metadata: false,
        supports_search_tool: false,
        multi_agent_version: None,
    }
}

fn host_for_entry<'a>(
    config: &'a Config,
    entry: &LocalModelEntry,
) -> Option<&'a LocalModelHostConfig> {
    entry
        .host_id
        .as_ref()
        .and_then(|host_id| config.local_model_hosts.get(host_id))
}

fn local_model_context_window(host: Option<&LocalModelHostConfig>) -> i64 {
    host_metadata_i64(host, "context_size")
        .or_else(|| host_metadata_i64(host, "ctx_size"))
        .unwrap_or(DEFAULT_LOCAL_MODEL_CONTEXT_WINDOW)
}

fn host_metadata_i64(host: Option<&LocalModelHostConfig>, key: &str) -> Option<i64> {
    host.and_then(|host| host.metadata.get(key))
        .and_then(|value| {
            value
                .as_i64()
                .or_else(|| value.as_u64().and_then(|value| i64::try_from(value).ok()))
        })
        .filter(|value| *value > 0)
}

fn native_local_base_instructions() -> String {
    NATIVE_LOCAL_BASE_INSTRUCTIONS.to_string()
}

fn discover_model_dir(root: &Path, scan_max_depth: usize) -> Vec<LocalModelEntry> {
    if !root.exists() {
        return Vec::new();
    }

    let mut seen_paths = BTreeSet::<PathBuf>::new();
    let mut entries = Vec::new();
    for entry in WalkDir::new(root)
        .follow_links(false)
        .max_depth(scan_max_depth.saturating_add(1))
        .into_iter()
        .filter_map(Result::ok)
    {
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        if !is_supported_model_file(path) {
            continue;
        }
        let key = canonical_or_raw(path);
        if !seen_paths.insert(key) {
            continue;
        }
        if let Some(model) = model_entry_from_path(path, Some(root), None, None, None) {
            entries.push(model);
        }
    }
    entries
}

pub(crate) fn resolve_local_model_from_runtime_config(
    config: &NativeLocalModelConfig,
    model: &str,
) -> Option<LocalModelEntry> {
    discover_local_models_from_runtime_config(config)
        .into_iter()
        .find(|entry| entry_matches_model(entry, model))
}

fn explicit_local_host_models(config: &NativeLocalModelConfig) -> Vec<LocalModelEntry> {
    let mut entries = Vec::new();
    for (host_id, host) in &config.local_model_hosts {
        let Some(model_path) = host.model_path.as_ref() else {
            continue;
        };
        let mut aliases = host
            .models
            .iter()
            .map(|model| model.trim())
            .filter(|model| !model.is_empty())
            .map(ToOwned::to_owned)
            .collect::<Vec<_>>();
        aliases.push(host_id.clone());
        let tokenizer_path = host.tokenizer_path.clone();
        if let Some(mut entry) = model_entry_from_path(
            model_path.as_path(),
            None,
            tokenizer_path,
            Some(host_id.clone()),
            Some(host.kind),
        ) {
            entry.aliases.extend(aliases);
            entry.aliases.sort();
            entry.aliases.dedup();
            entries.push(entry);
        }
    }
    entries
}

fn model_entry_from_path(
    path: &Path,
    root: Option<&Path>,
    tokenizer_path: Option<AbsolutePathBuf>,
    host_id: Option<String>,
    host_kind: Option<LocalModelHostKind>,
) -> Option<LocalModelEntry> {
    if is_gguf_projection_file(path) {
        return None;
    }
    let format = LocalModelFormat::from_model_path(path)?;
    let model_path = AbsolutePathBuf::from_absolute_path(path).ok()?;
    let display_name = display_name_from_path(path);
    let model_id = local_model_id(path, &display_name);
    let mut aliases = vec![display_name.clone()];
    if let Some(stem) = path.file_stem().and_then(|stem| stem.to_str()) {
        aliases.push(stem.to_string());
    }
    aliases.sort();
    aliases.dedup();
    let tokenizer_path =
        tokenizer_path.or_else(|| root.and_then(|root| find_tokenizer(path, root)));
    let gguf_metadata = if format == LocalModelFormat::Gguf {
        inspect_gguf_metadata(path)
    } else {
        None
    };
    let architecture = gguf_metadata
        .as_ref()
        .map(|metadata| metadata.architecture)
        .unwrap_or_else(|| LocalModelArchitecture::infer(path));
    let wire = local_model_wire(format, host_kind);
    let runtime_supported = wire != LocalModelWire::Unsupported;

    Some(LocalModelEntry {
        model_id,
        display_name,
        aliases,
        model_path,
        tokenizer_path,
        format,
        architecture,
        size_bytes: fs::metadata(path).ok().map(|metadata| metadata.len()),
        wire,
        host_id,
        runtime_supported,
    })
}

impl LocalModelFormat {
    fn from_model_path(path: &Path) -> Option<Self> {
        let file_name = path.file_name()?.to_string_lossy().to_ascii_lowercase();
        if file_name.ends_with(".gguf") {
            return Some(Self::Gguf);
        }
        if file_name == "model.safetensors" || file_name == "model.safetensors.index.json" {
            return Some(Self::SafeTensors);
        }
        None
    }
}

impl LocalModelArchitecture {
    fn infer(path: &Path) -> Self {
        let lower = path.to_string_lossy().to_ascii_lowercase();
        if lower.contains("gemma") {
            Self::Gemma
        } else if lower.contains("qwen3") || lower.contains("qwen-3") {
            Self::Qwen3
        } else if lower.contains("qwen2") || lower.contains("qwen-2") {
            Self::Qwen2
        } else if lower.contains("mistral") {
            Self::Mistral
        } else if lower.contains("llama") {
            Self::Llama
        } else {
            Self::Unknown
        }
    }

    fn from_gguf_architecture(raw: &str) -> Self {
        let lower = raw.to_ascii_lowercase();
        if lower.contains("gemma") {
            Self::Gemma
        } else if lower.contains("qwen3") {
            Self::Qwen3
        } else if lower.contains("qwen2") || lower.contains("qwen-2") {
            Self::Qwen2
        } else if lower.contains("mistral") {
            Self::Mistral
        } else if lower.contains("llama") {
            Self::Llama
        } else {
            Self::Unknown
        }
    }
}

fn local_model_wire(
    format: LocalModelFormat,
    host_kind: Option<LocalModelHostKind>,
) -> LocalModelWire {
    if format != LocalModelFormat::Gguf {
        return LocalModelWire::Unsupported;
    }
    match host_kind {
        Some(LocalModelHostKind::ExternalHttp) => LocalModelWire::ExternalOpenAiCompat,
        Some(LocalModelHostKind::ManagedServer) | None => LocalModelWire::LlamaCppGpu,
        Some(LocalModelHostKind::NativeEngine) => LocalModelWire::Unsupported,
    }
}

fn is_supported_model_file(path: &Path) -> bool {
    LocalModelFormat::from_model_path(path).is_some()
}

fn is_gguf_projection_file(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(|name| name.to_ascii_lowercase().contains("mmproj"))
        .unwrap_or(false)
}

#[derive(Debug, Clone, Copy)]
struct GgufMetadata {
    architecture: LocalModelArchitecture,
}

fn inspect_gguf_metadata(path: &Path) -> Option<GgufMetadata> {
    let mut file = fs::File::open(path).ok()?;
    let content = Content::read(&mut file).ok()?;
    let architecture = content
        .metadata
        .get("general.architecture")
        .and_then(value_string)
        .map(LocalModelArchitecture::from_gguf_architecture)
        .unwrap_or(LocalModelArchitecture::Unknown);
    Some(GgufMetadata { architecture })
}

fn value_string(value: &GgufValue) -> Option<&str> {
    value.to_string().ok().map(String::as_str)
}

fn entry_matches_model(entry: &LocalModelEntry, model: &str) -> bool {
    entry.model_id == model
        || entry.aliases.iter().any(|alias| alias == model)
        || entry.model_path.to_string_lossy() == model
}

fn find_tokenizer(model_path: &Path, root: &Path) -> Option<AbsolutePathBuf> {
    let model_dir = model_path.parent()?;
    for dir in model_dir.ancestors() {
        if !dir.starts_with(root) {
            break;
        }
        let candidate = dir.join(TOKENIZER_FILE_NAME);
        if candidate.is_file() {
            return AbsolutePathBuf::from_absolute_path(candidate).ok();
        }
    }
    None
}

fn display_name_from_path(path: &Path) -> String {
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .map(|stem| stem.trim_end_matches(".safetensors").to_string())
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(|| "local-model".to_string())
}

fn local_model_id(path: &Path, display_name: &str) -> String {
    format!(
        "local/{}-{}",
        sanitize_model_id(display_name),
        short_path_hash(path)
    )
}

fn sanitize_model_id(raw: &str) -> String {
    let mut out = String::new();
    for ch in raw.chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-') {
            out.push(ch.to_ascii_lowercase());
        } else if ch.is_whitespace() || matches!(ch, '/' | '\\') {
            out.push('-');
        }
    }
    let out = out.trim_matches('-').to_string();
    if out.is_empty() {
        "model".to_string()
    } else {
        out
    }
}

fn short_path_hash(path: &Path) -> String {
    let mut hasher = Sha1::new();
    hasher.update(path.to_string_lossy().as_bytes());
    let digest = hasher.finalize();
    digest[..4]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn model_path_key(entry: &LocalModelEntry) -> PathBuf {
    canonical_or_raw(entry.model_path.as_path())
}

fn canonical_or_raw(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}
