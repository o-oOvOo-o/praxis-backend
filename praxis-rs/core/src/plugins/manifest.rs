use praxis_utils_absolute_path::AbsolutePathBuf;
use praxis_utils_plugins::PLUGIN_MANIFEST_PATH;
use serde::Deserialize;
use serde_json::Value as JsonValue;
use std::collections::BTreeMap;
use std::fs;
use std::path::Component;
use std::path::Path;
const MAX_DEFAULT_PROMPT_COUNT: usize = 3;
const MAX_DEFAULT_PROMPT_LEN: usize = 128;
const MAX_PLUGIN_COMMAND_TIMEOUT_MS: u64 = 30 * 60 * 1_000;

pub use praxis_plugin::PluginLlmManifest as PluginManifestLlm;
pub use praxis_plugin::PluginLlmModel as PluginManifestLlmModel;
pub use praxis_plugin::PluginLlmModelCatalog as PluginManifestLlmModelCatalog;
pub use praxis_plugin::PluginLlmProduct as PluginManifestLlmProduct;
pub use praxis_plugin::PluginLlmProfile as PluginManifestLlmProfile;
pub use praxis_plugin::PluginLlmPromptSlot as PluginManifestLlmPromptSlot;
pub use praxis_plugin::PluginLlmToolPolicy as PluginManifestToolPolicy;

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawPluginManifest {
    #[serde(default)]
    name: String,
    #[serde(default)]
    description: Option<String>,
    // Keep manifest paths as raw strings so we can validate the required `./...` syntax before
    // resolving them under the plugin root.
    #[serde(default)]
    skills: Option<String>,
    #[serde(default)]
    mcp_servers: Option<String>,
    #[serde(default)]
    apps: Option<String>,
    #[serde(default)]
    interface: Option<RawPluginManifestInterface>,
    #[serde(default)]
    llm: Option<RawPluginManifestLlm>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginManifest {
    pub name: String,
    pub description: Option<String>,
    pub paths: PluginManifestPaths,
    pub interface: Option<PluginManifestInterface>,
    pub llm: Option<PluginManifestLlm>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginManifestPaths {
    pub skills: Option<AbsolutePathBuf>,
    pub mcp_servers: Option<AbsolutePathBuf>,
    pub apps: Option<AbsolutePathBuf>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PluginManifestInterface {
    pub display_name: Option<String>,
    pub short_description: Option<String>,
    pub long_description: Option<String>,
    pub developer_name: Option<String>,
    pub category: Option<String>,
    pub capabilities: Vec<String>,
    pub website_url: Option<String>,
    pub privacy_policy_url: Option<String>,
    pub terms_of_service_url: Option<String>,
    pub default_prompt: Option<Vec<String>>,
    pub brand_color: Option<String>,
    pub composer_icon: Option<AbsolutePathBuf>,
    pub logo: Option<AbsolutePathBuf>,
    pub screenshots: Vec<AbsolutePathBuf>,
    pub commands: Vec<PluginManifestCommand>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginManifestCommand {
    pub name: String,
    pub description: Option<String>,
    pub action: PluginManifestCommandAction,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PluginManifestCommandAction {
    Process {
        command: String,
        args: Vec<String>,
        cwd: Option<AbsolutePathBuf>,
        timeout_ms: Option<u64>,
    },
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawPluginManifestInterface {
    #[serde(default)]
    display_name: Option<String>,
    #[serde(default)]
    short_description: Option<String>,
    #[serde(default)]
    long_description: Option<String>,
    #[serde(default)]
    developer_name: Option<String>,
    #[serde(default)]
    category: Option<String>,
    #[serde(default)]
    capabilities: Vec<String>,
    #[serde(default)]
    #[serde(alias = "websiteURL")]
    website_url: Option<String>,
    #[serde(default)]
    #[serde(alias = "privacyPolicyURL")]
    privacy_policy_url: Option<String>,
    #[serde(default)]
    #[serde(alias = "termsOfServiceURL")]
    terms_of_service_url: Option<String>,
    #[serde(default)]
    default_prompt: Option<RawPluginManifestDefaultPrompt>,
    #[serde(default)]
    brand_color: Option<String>,
    #[serde(default)]
    composer_icon: Option<String>,
    #[serde(default)]
    logo: Option<String>,
    #[serde(default)]
    screenshots: Vec<String>,
    #[serde(default)]
    commands: Vec<RawPluginManifestCommand>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawPluginManifestCommand {
    #[serde(default)]
    name: String,
    #[serde(default)]
    description: Option<String>,
    action: Option<RawPluginManifestCommandAction>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
enum RawPluginManifestCommandAction {
    Process {
        #[serde(default)]
        command: String,
        #[serde(default)]
        args: Vec<String>,
        #[serde(default)]
        cwd: Option<String>,
        #[serde(default)]
        timeout_ms: Option<u64>,
    },
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawPluginManifestLlm {
    #[serde(default)]
    profiles: Vec<RawPluginManifestLlmProfile>,
    #[serde(default)]
    products: Vec<RawPluginManifestLlmProduct>,
    #[serde(default)]
    tool_policies: Vec<RawPluginManifestToolPolicy>,
    #[serde(default, alias = "model_catalogs")]
    model_catalogs: Vec<RawPluginManifestModelCatalog>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawPluginManifestLlmProfile {
    #[serde(default)]
    id: String,
    #[serde(default)]
    provider: Option<String>,
    #[serde(default)]
    wire: Option<String>,
    #[serde(default)]
    behavior: Option<String>,
    #[serde(default)]
    prompts: BTreeMap<String, String>,
    #[serde(default)]
    tasks: Option<String>,
    #[serde(default)]
    tools: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawPluginManifestLlmProduct {
    #[serde(default)]
    id: String,
    #[serde(default)]
    prompts: BTreeMap<String, String>,
    #[serde(default)]
    tasks: Option<String>,
    #[serde(default)]
    tools: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawPluginManifestToolPolicy {
    #[serde(default)]
    id: String,
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    applies_to: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawPluginManifestModelCatalog {
    #[serde(default)]
    id: String,
    #[serde(default)]
    label: Option<String>,
    #[serde(default)]
    provider: Option<String>,
    #[serde(default)]
    wire: Option<String>,
    #[serde(default)]
    models: Vec<RawPluginManifestModel>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawPluginManifestModel {
    #[serde(default, alias = "model")]
    slug: String,
    #[serde(default)]
    display_name: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    priority: Option<i32>,
    #[serde(default)]
    context_window: Option<i64>,
    #[serde(default)]
    default_reasoning_effort: Option<praxis_protocol::openai_models::ReasoningEffort>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum RawPluginManifestDefaultPrompt {
    String(String),
    List(Vec<RawPluginManifestDefaultPromptEntry>),
    Invalid(JsonValue),
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum RawPluginManifestDefaultPromptEntry {
    String(String),
    Invalid(JsonValue),
}

pub fn load_plugin_manifest(plugin_root: &Path) -> Option<PluginManifest> {
    let manifest_path = plugin_root.join(PLUGIN_MANIFEST_PATH);
    if !manifest_path.is_file() {
        return None;
    }
    let contents = fs::read_to_string(&manifest_path).ok()?;
    match serde_json::from_str::<RawPluginManifest>(&contents) {
        Ok(manifest) => {
            let RawPluginManifest {
                name: raw_name,
                description,
                skills,
                mcp_servers,
                apps,
                interface,
                llm,
            } = manifest;
            let name = plugin_root
                .file_name()
                .and_then(|entry| entry.to_str())
                .filter(|_| raw_name.trim().is_empty())
                .unwrap_or(&raw_name)
                .to_string();
            let interface = interface.and_then(|interface| {
                let RawPluginManifestInterface {
                    display_name,
                    short_description,
                    long_description,
                    developer_name,
                    category,
                    capabilities,
                    website_url,
                    privacy_policy_url,
                    terms_of_service_url,
                    default_prompt,
                    brand_color,
                    composer_icon,
                    logo,
                    screenshots,
                    commands,
                } = interface;

                let interface = PluginManifestInterface {
                    display_name,
                    short_description,
                    long_description,
                    developer_name,
                    category,
                    capabilities,
                    website_url,
                    privacy_policy_url,
                    terms_of_service_url,
                    default_prompt: resolve_default_prompts(plugin_root, default_prompt.as_ref()),
                    brand_color,
                    composer_icon: resolve_interface_asset_path(
                        plugin_root,
                        "interface.composerIcon",
                        composer_icon.as_deref(),
                    ),
                    logo: resolve_interface_asset_path(
                        plugin_root,
                        "interface.logo",
                        logo.as_deref(),
                    ),
                    screenshots: screenshots
                        .iter()
                        .filter_map(|screenshot| {
                            resolve_interface_asset_path(
                                plugin_root,
                                "interface.screenshots",
                                Some(screenshot),
                            )
                        })
                        .collect(),
                    commands: resolve_plugin_commands(plugin_root, commands),
                };

                let has_fields = interface.display_name.is_some()
                    || interface.short_description.is_some()
                    || interface.long_description.is_some()
                    || interface.developer_name.is_some()
                    || interface.category.is_some()
                    || !interface.capabilities.is_empty()
                    || interface.website_url.is_some()
                    || interface.privacy_policy_url.is_some()
                    || interface.terms_of_service_url.is_some()
                    || interface.default_prompt.is_some()
                    || interface.brand_color.is_some()
                    || interface.composer_icon.is_some()
                    || interface.logo.is_some()
                    || !interface.screenshots.is_empty()
                    || !interface.commands.is_empty();

                has_fields.then_some(interface)
            });
            Some(PluginManifest {
                name,
                description,
                paths: PluginManifestPaths {
                    skills: resolve_manifest_path(plugin_root, "skills", skills.as_deref()),
                    mcp_servers: resolve_manifest_path(
                        plugin_root,
                        "mcpServers",
                        mcp_servers.as_deref(),
                    ),
                    apps: resolve_manifest_path(plugin_root, "apps", apps.as_deref()),
                },
                interface,
                llm: resolve_llm_manifest(plugin_root, llm),
            })
        }
        Err(err) => {
            tracing::warn!(
                path = %manifest_path.display(),
                "failed to parse plugin manifest: {err}"
            );
            None
        }
    }
}

fn resolve_interface_asset_path(
    plugin_root: &Path,
    field: &'static str,
    path: Option<&str>,
) -> Option<AbsolutePathBuf> {
    resolve_manifest_path(plugin_root, field, path)
}

fn resolve_plugin_commands(
    plugin_root: &Path,
    commands: Vec<RawPluginManifestCommand>,
) -> Vec<PluginManifestCommand> {
    commands
        .into_iter()
        .enumerate()
        .filter_map(|(index, command)| resolve_plugin_command(plugin_root, index, command))
        .collect()
}

fn resolve_plugin_command(
    plugin_root: &Path,
    index: usize,
    command: RawPluginManifestCommand,
) -> Option<PluginManifestCommand> {
    let field = format!("interface.commands[{index}]");
    let Some(name) = normalize_manifest_string(&command.name) else {
        warn_invalid_plugin_command(plugin_root, &field, "name must not be empty");
        return None;
    };
    if !is_valid_plugin_command_name(&name) {
        warn_invalid_plugin_command(
            plugin_root,
            &format!("{field}.name"),
            "name must be lower-case ASCII letters, numbers, or '-'",
        );
        return None;
    }

    let Some(action) = command.action else {
        warn_invalid_plugin_command(
            plugin_root,
            &format!("{field}.action"),
            "action is required",
        );
        return None;
    };
    let action = resolve_plugin_command_action(plugin_root, &field, action)?;

    Some(PluginManifestCommand {
        name,
        description: normalize_optional_manifest_string(command.description),
        action,
    })
}

fn resolve_plugin_command_action(
    plugin_root: &Path,
    field: &str,
    action: RawPluginManifestCommandAction,
) -> Option<PluginManifestCommandAction> {
    match action {
        RawPluginManifestCommandAction::Process {
            command,
            args,
            cwd,
            timeout_ms,
        } => {
            let Some(command) = normalize_manifest_string(&command) else {
                warn_invalid_plugin_command(
                    plugin_root,
                    &format!("{field}.action.command"),
                    "command must not be empty",
                );
                return None;
            };
            let cwd =
                resolve_manifest_path(plugin_root, &format!("{field}.action.cwd"), cwd.as_deref());
            let timeout_ms = timeout_ms
                .filter(|timeout_ms| *timeout_ms > 0)
                .map(|timeout_ms| timeout_ms.min(MAX_PLUGIN_COMMAND_TIMEOUT_MS));

            Some(PluginManifestCommandAction::Process {
                command,
                args,
                cwd,
                timeout_ms,
            })
        }
    }
}

fn is_valid_plugin_command_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

fn warn_invalid_plugin_command(plugin_root: &Path, field: &str, message: &str) {
    let manifest_path = plugin_root.join(PLUGIN_MANIFEST_PATH);
    tracing::warn!(
        path = %manifest_path.display(),
        "ignoring {field}: {message}"
    );
}

fn resolve_default_prompts(
    plugin_root: &Path,
    value: Option<&RawPluginManifestDefaultPrompt>,
) -> Option<Vec<String>> {
    match value? {
        RawPluginManifestDefaultPrompt::String(prompt) => {
            resolve_default_prompt_str(plugin_root, "interface.defaultPrompt", prompt)
                .map(|prompt| vec![prompt])
        }
        RawPluginManifestDefaultPrompt::List(values) => {
            let mut prompts = Vec::new();
            for (index, item) in values.iter().enumerate() {
                if prompts.len() >= MAX_DEFAULT_PROMPT_COUNT {
                    warn_invalid_default_prompt(
                        plugin_root,
                        "interface.defaultPrompt",
                        &format!("maximum of {MAX_DEFAULT_PROMPT_COUNT} prompts is supported"),
                    );
                    break;
                }

                match item {
                    RawPluginManifestDefaultPromptEntry::String(prompt) => {
                        let field = format!("interface.defaultPrompt[{index}]");
                        if let Some(prompt) =
                            resolve_default_prompt_str(plugin_root, &field, prompt)
                        {
                            prompts.push(prompt);
                        }
                    }
                    RawPluginManifestDefaultPromptEntry::Invalid(value) => {
                        let field = format!("interface.defaultPrompt[{index}]");
                        warn_invalid_default_prompt(
                            plugin_root,
                            &field,
                            &format!("expected a string, found {}", json_value_type(value)),
                        );
                    }
                }
            }

            (!prompts.is_empty()).then_some(prompts)
        }
        RawPluginManifestDefaultPrompt::Invalid(value) => {
            warn_invalid_default_prompt(
                plugin_root,
                "interface.defaultPrompt",
                &format!(
                    "expected a string or array of strings, found {}",
                    json_value_type(value)
                ),
            );
            None
        }
    }
}

fn resolve_default_prompt_str(plugin_root: &Path, field: &str, prompt: &str) -> Option<String> {
    let prompt = prompt.split_whitespace().collect::<Vec<_>>().join(" ");
    if prompt.is_empty() {
        warn_invalid_default_prompt(plugin_root, field, "prompt must not be empty");
        return None;
    }
    if prompt.chars().count() > MAX_DEFAULT_PROMPT_LEN {
        warn_invalid_default_prompt(
            plugin_root,
            field,
            &format!("prompt must be at most {MAX_DEFAULT_PROMPT_LEN} characters"),
        );
        return None;
    }
    Some(prompt)
}

fn warn_invalid_default_prompt(plugin_root: &Path, field: &str, message: &str) {
    let manifest_path = plugin_root.join(PLUGIN_MANIFEST_PATH);
    tracing::warn!(
        path = %manifest_path.display(),
        "ignoring {field}: {message}"
    );
}

fn json_value_type(value: &JsonValue) -> &'static str {
    match value {
        JsonValue::Null => "null",
        JsonValue::Bool(_) => "boolean",
        JsonValue::Number(_) => "number",
        JsonValue::String(_) => "string",
        JsonValue::Array(_) => "array",
        JsonValue::Object(_) => "object",
    }
}

fn resolve_llm_manifest(
    plugin_root: &Path,
    raw_llm: Option<RawPluginManifestLlm>,
) -> Option<PluginManifestLlm> {
    let raw_llm = raw_llm?;
    let profiles = raw_llm
        .profiles
        .into_iter()
        .enumerate()
        .filter_map(|(index, raw)| resolve_llm_profile(plugin_root, index, raw))
        .collect::<Vec<_>>();
    let products = raw_llm
        .products
        .into_iter()
        .enumerate()
        .filter_map(|(index, raw)| resolve_llm_product(plugin_root, index, raw))
        .collect::<Vec<_>>();
    let tool_policies = raw_llm
        .tool_policies
        .into_iter()
        .enumerate()
        .filter_map(|(index, raw)| resolve_llm_tool_policy(plugin_root, index, raw))
        .collect::<Vec<_>>();
    let model_catalogs = raw_llm
        .model_catalogs
        .into_iter()
        .enumerate()
        .filter_map(|(index, raw)| resolve_llm_model_catalog(plugin_root, index, raw))
        .collect::<Vec<_>>();

    (!profiles.is_empty()
        || !products.is_empty()
        || !tool_policies.is_empty()
        || !model_catalogs.is_empty())
    .then_some(PluginManifestLlm {
        profiles,
        products,
        tool_policies,
        model_catalogs,
    })
}

fn resolve_llm_profile(
    plugin_root: &Path,
    index: usize,
    raw: RawPluginManifestLlmProfile,
) -> Option<PluginManifestLlmProfile> {
    let field = format!("llm.profiles[{index}].id");
    let id = resolve_manifest_id(plugin_root, &field, &raw.id)?;
    Some(PluginManifestLlmProfile {
        id,
        provider: normalize_optional_manifest_string(raw.provider),
        wire: normalize_optional_manifest_string(raw.wire),
        behavior: resolve_manifest_path(
            plugin_root,
            &format!("llm.profiles[{index}].behavior"),
            raw.behavior.as_deref(),
        ),
        prompts: resolve_prompt_slots(
            plugin_root,
            &format!("llm.profiles[{index}].prompts"),
            raw.prompts,
        ),
        tasks: resolve_manifest_path(
            plugin_root,
            &format!("llm.profiles[{index}].tasks"),
            raw.tasks.as_deref(),
        ),
        tools: resolve_manifest_path(
            plugin_root,
            &format!("llm.profiles[{index}].tools"),
            raw.tools.as_deref(),
        ),
    })
}

fn resolve_llm_product(
    plugin_root: &Path,
    index: usize,
    raw: RawPluginManifestLlmProduct,
) -> Option<PluginManifestLlmProduct> {
    let field = format!("llm.products[{index}].id");
    let id = resolve_manifest_id(plugin_root, &field, &raw.id)?;
    Some(PluginManifestLlmProduct {
        id,
        prompts: resolve_prompt_slots(
            plugin_root,
            &format!("llm.products[{index}].prompts"),
            raw.prompts,
        ),
        tasks: resolve_manifest_path(
            plugin_root,
            &format!("llm.products[{index}].tasks"),
            raw.tasks.as_deref(),
        ),
        tools: resolve_manifest_path(
            plugin_root,
            &format!("llm.products[{index}].tools"),
            raw.tools.as_deref(),
        ),
    })
}

fn resolve_llm_tool_policy(
    plugin_root: &Path,
    index: usize,
    raw: RawPluginManifestToolPolicy,
) -> Option<PluginManifestToolPolicy> {
    let id = resolve_manifest_id(
        plugin_root,
        &format!("llm.toolPolicies[{index}].id"),
        &raw.id,
    )?;
    let path = resolve_manifest_path(
        plugin_root,
        &format!("llm.toolPolicies[{index}].path"),
        raw.path.as_deref(),
    )?;
    Some(PluginManifestToolPolicy {
        id,
        path,
        applies_to: raw
            .applies_to
            .into_iter()
            .filter_map(|value| normalize_manifest_string(&value))
            .collect(),
    })
}

fn resolve_llm_model_catalog(
    plugin_root: &Path,
    index: usize,
    raw: RawPluginManifestModelCatalog,
) -> Option<PluginManifestLlmModelCatalog> {
    let id = resolve_manifest_id(
        plugin_root,
        &format!("llm.modelCatalogs[{index}].id"),
        &raw.id,
    )?;
    let models = raw
        .models
        .into_iter()
        .enumerate()
        .filter_map(|(model_index, raw_model)| {
            resolve_llm_model(plugin_root, index, model_index, raw_model)
        })
        .collect::<Vec<_>>();
    if models.is_empty() {
        tracing::warn!(
            path = %plugin_root.join(PLUGIN_MANIFEST_PATH).display(),
            "ignoring llm.modelCatalogs[{index}]: models must not be empty"
        );
        return None;
    }

    Some(PluginManifestLlmModelCatalog {
        id,
        label: normalize_optional_manifest_string(raw.label),
        provider: normalize_optional_manifest_string(raw.provider),
        wire: normalize_optional_manifest_string(raw.wire),
        models,
    })
}

fn resolve_llm_model(
    plugin_root: &Path,
    catalog_index: usize,
    model_index: usize,
    raw: RawPluginManifestModel,
) -> Option<PluginManifestLlmModel> {
    let slug = resolve_manifest_id(
        plugin_root,
        &format!("llm.modelCatalogs[{catalog_index}].models[{model_index}].slug"),
        &raw.slug,
    )?;
    Some(PluginManifestLlmModel {
        slug,
        display_name: normalize_optional_manifest_string(raw.display_name),
        description: normalize_optional_manifest_string(raw.description),
        priority: raw.priority,
        context_window: raw.context_window,
        default_reasoning_effort: raw.default_reasoning_effort,
    })
}

fn resolve_prompt_slots(
    plugin_root: &Path,
    field: &str,
    prompts: BTreeMap<String, String>,
) -> Vec<PluginManifestLlmPromptSlot> {
    prompts
        .into_iter()
        .filter_map(|(slot, path)| {
            let slot = normalize_manifest_string(&slot)?;
            let field = format!("{field}.{slot}");
            resolve_manifest_path(plugin_root, &field, Some(path.as_str()))
                .map(|path| PluginManifestLlmPromptSlot { slot, path })
        })
        .collect()
}

fn resolve_manifest_id(plugin_root: &Path, field: &str, value: &str) -> Option<String> {
    let Some(value) = normalize_manifest_string(value) else {
        let manifest_path = plugin_root.join(PLUGIN_MANIFEST_PATH);
        tracing::warn!(
            path = %manifest_path.display(),
            "ignoring {field}: id must not be empty"
        );
        return None;
    };
    Some(value)
}

fn normalize_optional_manifest_string(value: Option<String>) -> Option<String> {
    value.and_then(|value| normalize_manifest_string(&value))
}

fn normalize_manifest_string(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_string())
}

fn resolve_manifest_path(
    plugin_root: &Path,
    field: &str,
    path: Option<&str>,
) -> Option<AbsolutePathBuf> {
    // `plugin.json` paths are required to be relative to the plugin root and we return the
    // normalized absolute path to the rest of the system.
    let path = path?;
    if path.is_empty() {
        return None;
    }
    let Some(relative_path) = path.strip_prefix("./") else {
        tracing::warn!("ignoring {field}: path must start with `./` relative to plugin root");
        return None;
    };
    if relative_path.is_empty() {
        tracing::warn!("ignoring {field}: path must not be `./`");
        return None;
    }

    let mut normalized = std::path::PathBuf::new();
    for component in Path::new(relative_path).components() {
        match component {
            Component::Normal(component) => normalized.push(component),
            Component::ParentDir => {
                tracing::warn!("ignoring {field}: path must not contain '..'");
                return None;
            }
            _ => {
                tracing::warn!("ignoring {field}: path must stay within the plugin root");
                return None;
            }
        }
    }

    AbsolutePathBuf::try_from(plugin_root.join(normalized))
        .map_err(|err| {
            tracing::warn!("ignoring {field}: path must resolve to an absolute path: {err}");
            err
        })
        .ok()
}

#[cfg(test)]
mod tests {
    use super::MAX_DEFAULT_PROMPT_LEN;
    use super::PluginManifest;
    use super::load_plugin_manifest;
    use pretty_assertions::assert_eq;
    use std::fs;
    use std::path::Path;
    use tempfile::tempdir;

    fn write_manifest(plugin_root: &Path, interface: &str) {
        write_raw_manifest(
            plugin_root,
            &format!(
                r#"{{
  "name": "demo-plugin",
  "interface": {interface}
}}"#
            ),
        );
    }

    fn write_raw_manifest(plugin_root: &Path, manifest: &str) {
        fs::create_dir_all(plugin_root.join(".praxis-plugin")).expect("create manifest dir");
        fs::write(plugin_root.join(".praxis-plugin/plugin.json"), manifest)
            .expect("write manifest");
    }

    fn load_manifest(plugin_root: &Path) -> PluginManifest {
        load_plugin_manifest(plugin_root).expect("load plugin manifest")
    }

    #[test]
    fn plugin_interface_accepts_legacy_default_prompt_string() {
        let tmp = tempdir().expect("tempdir");
        let plugin_root = tmp.path().join("demo-plugin");
        write_manifest(
            &plugin_root,
            r#"{
    "displayName": "Demo Plugin",
    "defaultPrompt": "  Summarize   my inbox  "
  }"#,
        );

        let manifest = load_manifest(&plugin_root);
        let interface = manifest.interface.expect("plugin interface");

        assert_eq!(
            interface.default_prompt,
            Some(vec!["Summarize my inbox".to_string()])
        );
    }

    #[test]
    fn plugin_interface_normalizes_default_prompt_array() {
        let tmp = tempdir().expect("tempdir");
        let plugin_root = tmp.path().join("demo-plugin");
        let too_long = "x".repeat(MAX_DEFAULT_PROMPT_LEN + 1);
        write_manifest(
            &plugin_root,
            &format!(
                r#"{{
    "displayName": "Demo Plugin",
    "defaultPrompt": [
      " Summarize my inbox ",
      123,
      "{too_long}",
      "   ",
      "Draft the reply  ",
      "Find   my next action",
      "Archive old mail"
    ]
  }}"#
            ),
        );

        let manifest = load_manifest(&plugin_root);
        let interface = manifest.interface.expect("plugin interface");

        assert_eq!(
            interface.default_prompt,
            Some(vec![
                "Summarize my inbox".to_string(),
                "Draft the reply".to_string(),
                "Find my next action".to_string(),
            ])
        );
    }

    #[test]
    fn plugin_interface_ignores_invalid_default_prompt_shape() {
        let tmp = tempdir().expect("tempdir");
        let plugin_root = tmp.path().join("demo-plugin");
        write_manifest(
            &plugin_root,
            r#"{
    "displayName": "Demo Plugin",
    "defaultPrompt": { "text": "Summarize my inbox" }
  }"#,
        );

        let manifest = load_manifest(&plugin_root);
        let interface = manifest.interface.expect("plugin interface");

        assert_eq!(interface.default_prompt, None);
    }

    #[test]
    fn plugin_manifest_resolves_llm_profile_product_and_tool_policy_paths() {
        let tmp = tempdir().expect("tempdir");
        let plugin_root = tmp.path().join("demo-plugin");
        write_raw_manifest(
            &plugin_root,
            r#"{
  "name": "demo-plugin",
  "llm": {
    "profiles": [
      {
        "id": "deepseek",
        "provider": "deepseek",
        "wire": "openai_compat",
        "behavior": "./llm/deepseek/behavior.toml",
        "prompts": {
          "base": "./llm/deepseek/prompts/base.md"
        },
        "tasks": "./llm/deepseek/tasks.toml",
        "tools": "./llm/deepseek/tools.toml"
      }
    ],
    "products": [
      {
        "id": "cunning3d",
        "prompts": {
          "base": "./llm/products/cunning3d/base.md"
        }
      }
    ],
    "toolPolicies": [
      {
        "id": "c3d-dev",
        "path": "./llm/tools/c3d-dev.toml",
        "appliesTo": ["deepseek", "openai/responses"]
      }
    ],
    "modelCatalogs": [
      {
        "id": "aliyun-coder",
        "label": "Aliyun Coder",
        "provider": "dashscope",
        "wire": "openai_compat",
        "models": [
          {
            "slug": "qwen3-coder-plus",
            "displayName": "Qwen3 Coder Plus",
            "description": "Aliyun coding model",
            "priority": 20,
            "contextWindow": 262144,
            "defaultReasoningEffort": "high"
          }
        ]
      }
    ]
  }
}"#,
        );

        let manifest = load_manifest(&plugin_root);
        let llm = manifest.llm.expect("plugin llm manifest");

        assert_eq!(llm.profiles[0].id, "deepseek");
        assert_eq!(llm.profiles[0].provider.as_deref(), Some("deepseek"));
        assert_eq!(llm.profiles[0].wire.as_deref(), Some("openai_compat"));
        assert!(
            llm.profiles[0].prompts[0]
                .path
                .as_path()
                .ends_with("llm/deepseek/prompts/base.md")
        );
        assert_eq!(llm.products[0].id, "cunning3d");
        assert!(
            llm.products[0].prompts[0]
                .path
                .as_path()
                .ends_with("llm/products/cunning3d/base.md")
        );
        assert_eq!(llm.tool_policies[0].id, "c3d-dev");
        assert_eq!(
            llm.tool_policies[0].applies_to,
            vec!["deepseek".to_string(), "openai/responses".to_string()]
        );
        assert_eq!(llm.model_catalogs[0].id, "aliyun-coder");
        assert_eq!(llm.model_catalogs[0].provider.as_deref(), Some("dashscope"));
        assert_eq!(llm.model_catalogs[0].models[0].slug, "qwen3-coder-plus");
        assert_eq!(
            llm.model_catalogs[0].models[0].display_name.as_deref(),
            Some("Qwen3 Coder Plus")
        );
    }
}
