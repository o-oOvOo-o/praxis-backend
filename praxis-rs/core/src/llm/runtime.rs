use std::collections::HashSet;
use std::fs;

use praxis_plugin::PluginLlmManifest;
use praxis_plugin::PluginLlmModel;
use praxis_plugin::PluginLlmModelCatalog;
use praxis_plugin::PluginLlmProduct;
use praxis_plugin::PluginLlmProfile;
use praxis_plugin::PluginLlmPromptSlot;
use praxis_plugin::PluginLlmToolPolicy;

use crate::llm::ids::BehaviorProfileId;
use crate::llm::ids::ProductProfileId;
use crate::llm::profiles::plugin::ProfileAutoTitleModel;
use crate::llm::profiles::plugin::ProfileDescriptor;
use crate::llm::profiles::plugin::ProfileMatchContext;
use crate::llm::profiles::plugin::ProfileTaskPolicyDescriptor;
use crate::llm::prompts::LlmPromptPurpose;
use crate::llm::registry::LlmProfileRegistry;
use crate::llm::tasks::compact::CompactExecutionPolicy;
use crate::model_provider_info::ModelProviderInfo;
use praxis_protocol::config_types::ReasoningSummary;
use praxis_protocol::config_types::Verbosity;
use praxis_protocol::models::BASE_INSTRUCTIONS_DEFAULT;
use praxis_protocol::openai_models::ApplyPatchToolType;
use praxis_protocol::openai_models::ConfigShellToolType;
use praxis_protocol::openai_models::ModelInfo;
use praxis_protocol::openai_models::ModelVisibility;
use praxis_protocol::openai_models::ReasoningEffort;
use praxis_protocol::openai_models::TruncationPolicyConfig;
use praxis_protocol::openai_models::WebSearchToolType;
use praxis_protocol::openai_models::default_input_modalities;
use praxis_protocol::openai_models::known_openai_compatible_model_info;
use praxis_protocol::openai_models::provider_neutral_reasoning_levels;
use praxis_tools::ToolCapabilityConfig;
use praxis_tools::ToolWebSearchBackend;
use serde::Deserialize;

#[derive(Debug, Clone, Default)]
pub(crate) struct LlmRuntimeCatalog {
    plugin_manifests: Vec<PluginLlmManifest>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct LlmToolVisibilityPolicy {
    visible_tools: Option<HashSet<String>>,
    hidden_tools: HashSet<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawLlmToolVisibilityPolicy {
    #[serde(
        default,
        alias = "visible_tools",
        alias = "allowed_tools",
        alias = "allow"
    )]
    visible_tools: Option<Vec<String>>,
    #[serde(
        default,
        alias = "hidden_tools",
        alias = "denied_tools",
        alias = "deny"
    )]
    hidden_tools: Vec<String>,
    #[serde(
        default,
        alias = "web_search",
        alias = "webSearch",
        alias = "web_search_backend",
        alias = "webSearchBackend"
    )]
    web_search_backend: Option<RawLlmWebSearchBackend>,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum RawLlmWebSearchBackend {
    Responses,
    Praxis,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct LlmAutoTitleTaskPolicy {
    pub(crate) model_slug: Option<String>,
    pub(crate) reasoning_effort: Option<ReasoningEffort>,
    pub(crate) suppress_model_default_reasoning: Option<bool>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct LlmTaskPolicy {
    auto_title: Option<LlmAutoTitleTaskPolicy>,
    compact_execution: Option<CompactExecutionPolicy>,
    compact_model: Option<String>,
    auto_compact_token_limit_cap: Option<i64>,
}

impl LlmAutoTitleTaskPolicy {
    fn merge(&mut self, other: Self) {
        if other.model_slug.is_some() {
            self.model_slug = other.model_slug;
        }
        if other.reasoning_effort.is_some() {
            self.reasoning_effort = other.reasoning_effort;
        }
        if other.suppress_model_default_reasoning.is_some() {
            self.suppress_model_default_reasoning = other.suppress_model_default_reasoning;
        }
    }
}

impl LlmTaskPolicy {
    fn from_profile_descriptor(descriptor: ProfileTaskPolicyDescriptor) -> Self {
        Self {
            auto_title: descriptor
                .auto_title
                .map(|auto_title| LlmAutoTitleTaskPolicy {
                    model_slug: match auto_title.model {
                        ProfileAutoTitleModel::Current => None,
                        ProfileAutoTitleModel::Fixed(model_slug) => Some(model_slug.to_string()),
                    },
                    reasoning_effort: auto_title.reasoning_effort,
                    suppress_model_default_reasoning: Some(
                        auto_title.suppress_model_default_reasoning,
                    ),
                }),
            compact_execution: descriptor.compact_execution,
            compact_model: descriptor.compact_model.map(str::to_string),
            auto_compact_token_limit_cap: descriptor.auto_compact_token_limit_cap,
        }
    }

    fn is_empty(&self) -> bool {
        self.auto_title.is_none()
            && self.compact_execution.is_none()
            && self.compact_model.is_none()
            && self.auto_compact_token_limit_cap.is_none()
    }

    fn merge(&mut self, other: Self) {
        if let Some(other_auto_title) = other.auto_title {
            self.auto_title
                .get_or_insert_with(LlmAutoTitleTaskPolicy::default)
                .merge(other_auto_title);
        }
        if other.compact_execution.is_some() {
            self.compact_execution = other.compact_execution;
        }
        if other.compact_model.is_some() {
            self.compact_model = other.compact_model;
        }
        if other.auto_compact_token_limit_cap.is_some() {
            self.auto_compact_token_limit_cap = other.auto_compact_token_limit_cap;
        }
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawLlmTaskPolicy {
    #[serde(default, alias = "auto_title")]
    auto_title: Option<RawAutoTitleTaskPolicy>,
    #[serde(default)]
    compact: Option<RawCompactTaskPolicy>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawAutoTitleTaskPolicy {
    #[serde(default, alias = "model_slug")]
    model: Option<String>,
    #[serde(default, alias = "reasoning_effort")]
    reasoning_effort: Option<ReasoningEffort>,
    #[serde(
        default,
        alias = "suppress_model_default_reasoning",
        alias = "suppressDefaultReasoning"
    )]
    suppress_model_default_reasoning: Option<bool>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawCompactTaskPolicy {
    #[serde(default)]
    execution: Option<String>,
    #[serde(
        default,
        alias = "model_slug",
        alias = "modelSlug",
        alias = "compact_model",
        alias = "compactModel"
    )]
    model: Option<String>,
    #[serde(
        default,
        alias = "auto_compact_token_limit",
        alias = "autoCompactTokenLimit",
        alias = "token_limit_cap",
        alias = "tokenLimitCap"
    )]
    auto_compact_token_limit_cap: Option<i64>,
}

impl LlmToolVisibilityPolicy {
    #[cfg(test)]
    pub(crate) fn from_tool_names(visible_tools: Option<&[&str]>, hidden_tools: &[&str]) -> Self {
        Self {
            visible_tools: visible_tools.map(|tools| {
                tools
                    .iter()
                    .filter_map(|tool| normalize_non_empty_tool_name(tool))
                    .collect()
            }),
            hidden_tools: hidden_tools
                .iter()
                .filter_map(|tool| normalize_non_empty_tool_name(tool))
                .collect(),
        }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.visible_tools
            .as_ref()
            .is_none_or(|tools| tools.is_empty())
            && self.hidden_tools.is_empty()
    }

    pub(crate) fn allows(&self, tool_name: &str) -> bool {
        let tool_name = normalize_tool_name(tool_name);
        if tool_name.is_empty() {
            return false;
        }
        if self.hidden_tools.contains(&tool_name) {
            return false;
        }
        self.visible_tools
            .as_ref()
            .is_none_or(|visible_tools| visible_tools.contains(&tool_name))
    }

    fn merge(&mut self, other: Self) {
        if let Some(other_visible_tools) = other.visible_tools {
            self.visible_tools
                .get_or_insert_with(HashSet::new)
                .extend(other_visible_tools);
        }
        self.hidden_tools.extend(other.hidden_tools);
    }
}

impl LlmRuntimeCatalog {
    pub(crate) fn from_plugin_manifests(plugin_manifests: Vec<PluginLlmManifest>) -> Self {
        Self { plugin_manifests }
    }

    pub(crate) fn model_infos_for_provider(
        &self,
        provider_id: &str,
        provider: &ModelProviderInfo,
    ) -> Vec<ModelInfo> {
        let mut seen = HashSet::<String>::new();
        let mut models = Vec::new();
        for catalog in self
            .plugin_manifests
            .iter()
            .flat_map(|manifest| manifest.model_catalogs.iter())
            .filter(|catalog| plugin_model_catalog_matches(catalog, provider_id, provider))
        {
            for (index, model) in catalog.models.iter().enumerate() {
                let slug = normalize_non_empty_string(&model.slug);
                let Some(slug) = slug else {
                    continue;
                };
                if seen.insert(slug.clone()) {
                    models.push(plugin_model_info(model, slug, index as i32));
                }
            }
        }
        models.sort_by(|left, right| left.priority.cmp(&right.priority));
        models
    }

    pub(crate) fn resolve_builtin_profile(
        &self,
        model_info: &ModelInfo,
        provider_id: &str,
        provider: &ModelProviderInfo,
    ) -> Option<ProfileDescriptor> {
        let ctx = ProfileMatchContext {
            model_info,
            provider_id,
            provider,
        };
        LlmProfileRegistry::builtin_static().resolve(&ctx)
    }

    pub(crate) fn resolve_profile_prompt_for_model(
        &self,
        model_info: &ModelInfo,
        provider_id: &str,
        provider: &ModelProviderInfo,
        purpose: LlmPromptPurpose,
    ) -> Option<String> {
        let profile = self.resolve_builtin_profile(model_info, provider_id, provider)?;
        self.resolve_profile_prompt(profile, provider_id, provider, purpose)
    }

    pub(crate) fn resolve_prompt_for_model(
        &self,
        model_info: &ModelInfo,
        provider_id: &str,
        provider: &ModelProviderInfo,
        product: Option<ProductProfileId>,
        purpose: LlmPromptPurpose,
    ) -> Option<String> {
        join_optional_prompt_layers(
            self.resolve_profile_prompt_for_model(model_info, provider_id, provider, purpose),
            product.and_then(|product| self.resolve_product_prompt(product, purpose)),
        )
    }

    pub(crate) fn resolve_profile_prompt(
        &self,
        profile: ProfileDescriptor,
        provider_id: &str,
        provider: &ModelProviderInfo,
        purpose: LlmPromptPurpose,
    ) -> Option<String> {
        self.plugin_manifests
            .iter()
            .flat_map(|manifest| manifest.profiles.iter())
            .filter(|plugin_profile| {
                plugin_profile_matches(plugin_profile, profile, provider_id, provider)
            })
            .find_map(|plugin_profile| {
                resolve_prompt_slot(plugin_profile.prompts.iter(), purpose).and_then(|slot| {
                    read_plugin_prompt(slot, plugin_profile.id.as_str(), profile.id, purpose)
                })
            })
    }

    pub(crate) fn resolve_product_prompt(
        &self,
        product: ProductProfileId,
        purpose: LlmPromptPurpose,
    ) -> Option<String> {
        self.plugin_manifests
            .iter()
            .flat_map(|manifest| manifest.products.iter())
            .filter(|plugin_product| product_id_matches(plugin_product, product))
            .find_map(|plugin_product| {
                resolve_prompt_slot(plugin_product.prompts.iter(), purpose).and_then(|slot| {
                    read_plugin_prompt(
                        slot,
                        plugin_product.id.as_str(),
                        product.as_behavior_id(),
                        purpose,
                    )
                })
            })
    }

    pub(crate) fn profile_task_policy_path(
        &self,
        profile: ProfileDescriptor,
        provider_id: &str,
        provider: &ModelProviderInfo,
    ) -> Option<praxis_utils_absolute_path::AbsolutePathBuf> {
        self.plugin_manifests
            .iter()
            .flat_map(|manifest| manifest.profiles.iter())
            .find(|plugin_profile| {
                plugin_profile_matches(plugin_profile, profile, provider_id, provider)
            })
            .and_then(|plugin_profile| plugin_profile.tasks.clone())
    }

    pub(crate) fn profile_tools_policy_path(
        &self,
        profile: ProfileDescriptor,
        provider_id: &str,
        provider: &ModelProviderInfo,
    ) -> Option<praxis_utils_absolute_path::AbsolutePathBuf> {
        self.plugin_manifests
            .iter()
            .flat_map(|manifest| manifest.profiles.iter())
            .find(|plugin_profile| {
                plugin_profile_matches(plugin_profile, profile, provider_id, provider)
            })
            .and_then(|plugin_profile| plugin_profile.tools.clone())
    }

    pub(crate) fn product_task_policy_path(
        &self,
        product: ProductProfileId,
    ) -> Option<praxis_utils_absolute_path::AbsolutePathBuf> {
        self.plugin_manifests
            .iter()
            .flat_map(|manifest| manifest.products.iter())
            .find(|plugin_product| product_id_matches(plugin_product, product))
            .and_then(|plugin_product| plugin_product.tasks.clone())
    }

    pub(crate) fn product_tools_policy_path(
        &self,
        product: ProductProfileId,
    ) -> Option<praxis_utils_absolute_path::AbsolutePathBuf> {
        self.plugin_manifests
            .iter()
            .flat_map(|manifest| manifest.products.iter())
            .find(|plugin_product| product_id_matches(plugin_product, product))
            .and_then(|plugin_product| plugin_product.tools.clone())
    }

    pub(crate) fn tool_policies_for_profile(
        &self,
        profile: BehaviorProfileId,
    ) -> Vec<PluginLlmToolPolicy> {
        self.plugin_manifests
            .iter()
            .flat_map(|manifest| manifest.tool_policies.iter())
            .filter(|policy| {
                policy.applies_to.is_empty()
                    || policy
                        .applies_to
                        .iter()
                        .any(|selector| profile_id_matches(selector, profile))
            })
            .cloned()
            .collect()
    }

    pub(crate) fn tool_visibility_policy_for_model(
        &self,
        model_info: &ModelInfo,
        provider_id: &str,
        provider: &ModelProviderInfo,
        product: Option<ProductProfileId>,
    ) -> Option<LlmToolVisibilityPolicy> {
        let profile = self.resolve_builtin_profile(model_info, provider_id, provider)?;
        let mut policy = LlmToolVisibilityPolicy::default();

        if let Some(path) = self.profile_tools_policy_path(profile, provider_id, provider)
            && let Some(profile_policy) =
                read_tool_visibility_policy(path.as_path(), profile.id, "profile.tools")
        {
            policy.merge(profile_policy);
        }

        for tool_policy in self.tool_policies_for_profile(profile.id) {
            if let Some(tool_policy) = read_tool_visibility_policy(
                tool_policy.path.as_path(),
                profile.id,
                tool_policy.id.as_str(),
            ) {
                policy.merge(tool_policy);
            }
        }

        if let Some(product) = product
            && let Some(path) = self.product_tools_policy_path(product)
            && let Some(product_policy) = read_tool_visibility_policy(
                path.as_path(),
                product.as_behavior_id(),
                "product.tools",
            )
        {
            policy.merge(product_policy);
        }

        (!policy.is_empty()).then_some(policy)
    }

    pub(crate) fn tool_capabilities_for_model(
        &self,
        model_info: &ModelInfo,
        provider_id: &str,
        provider: &ModelProviderInfo,
        product: Option<ProductProfileId>,
    ) -> ToolCapabilityConfig {
        let Some(profile) = self.resolve_builtin_profile(model_info, provider_id, provider) else {
            return ToolCapabilityConfig::default();
        };
        let mut capabilities = ToolCapabilityConfig {
            web_search_backend: profile.tool_capabilities.web_search_backend,
        };

        if let Some(path) = self.profile_tools_policy_path(profile, provider_id, provider)
            && let Some(profile_capabilities) =
                read_tool_capability_policy(path.as_path(), profile.id, "profile.tools")
        {
            merge_tool_capabilities(&mut capabilities, profile_capabilities);
        }

        for tool_policy in self.tool_policies_for_profile(profile.id) {
            if let Some(tool_capabilities) = read_tool_capability_policy(
                tool_policy.path.as_path(),
                profile.id,
                tool_policy.id.as_str(),
            ) {
                merge_tool_capabilities(&mut capabilities, tool_capabilities);
            }
        }

        if let Some(product) = product
            && let Some(path) = self.product_tools_policy_path(product)
            && let Some(product_capabilities) = read_tool_capability_policy(
                path.as_path(),
                product.as_behavior_id(),
                "product.tools",
            )
        {
            merge_tool_capabilities(&mut capabilities, product_capabilities);
        }

        capabilities
    }

    pub(crate) fn auto_title_task_policy_for_model(
        &self,
        model_info: &ModelInfo,
        provider_id: &str,
        provider: &ModelProviderInfo,
        product: Option<ProductProfileId>,
    ) -> Option<LlmAutoTitleTaskPolicy> {
        self.task_policy_for_model(model_info, provider_id, provider, product)?
            .auto_title
    }

    pub(crate) fn compact_execution_policy_for_model(
        &self,
        model_info: &ModelInfo,
        provider_id: &str,
        provider: &ModelProviderInfo,
        product: Option<ProductProfileId>,
    ) -> Option<CompactExecutionPolicy> {
        self.task_policy_for_model(model_info, provider_id, provider, product)?
            .compact_execution
    }

    pub(crate) fn compact_model_for_model(
        &self,
        model_info: &ModelInfo,
        provider_id: &str,
        provider: &ModelProviderInfo,
        product: Option<ProductProfileId>,
    ) -> Option<String> {
        self.task_policy_for_model(model_info, provider_id, provider, product)?
            .compact_model
    }

    pub(crate) fn auto_compact_token_limit_cap_for_model(
        &self,
        model_info: &ModelInfo,
        provider_id: &str,
        provider: &ModelProviderInfo,
        product: Option<ProductProfileId>,
    ) -> Option<i64> {
        self.task_policy_for_model(model_info, provider_id, provider, product)?
            .auto_compact_token_limit_cap
    }

    fn task_policy_for_model(
        &self,
        model_info: &ModelInfo,
        provider_id: &str,
        provider: &ModelProviderInfo,
        product: Option<ProductProfileId>,
    ) -> Option<LlmTaskPolicy> {
        let profile = self.resolve_builtin_profile(model_info, provider_id, provider)?;
        let mut policy = LlmTaskPolicy::from_profile_descriptor(profile.task_policy);
        if let Some(path) = self.profile_task_policy_path(profile, provider_id, provider)
            && let Some(profile_policy) = read_task_policy(path.as_path(), profile.id)
        {
            policy.merge(profile_policy);
        }
        if let Some(product) = product
            && let Some(path) = self.product_task_policy_path(product)
            && let Some(product_policy) = read_task_policy(path.as_path(), product.as_behavior_id())
        {
            policy.merge(product_policy);
        }
        (!policy.is_empty()).then_some(policy)
    }
}

fn plugin_profile_matches(
    plugin_profile: &PluginLlmProfile,
    profile: ProfileDescriptor,
    provider_id: &str,
    provider: &ModelProviderInfo,
) -> bool {
    profile_id_matches(&plugin_profile.id, profile.id)
        && plugin_profile
            .provider
            .as_deref()
            .is_none_or(|provider_selector| {
                provider_selector_matches(provider_selector, provider_id, provider)
            })
        && plugin_profile
            .wire
            .as_deref()
            .is_none_or(|wire_selector| wire_selector_matches(wire_selector, provider))
}

fn plugin_model_catalog_matches(
    catalog: &PluginLlmModelCatalog,
    provider_id: &str,
    provider: &ModelProviderInfo,
) -> bool {
    catalog.provider.as_deref().is_none_or(|provider_selector| {
        provider_selector_matches(provider_selector, provider_id, provider)
    }) && catalog
        .wire
        .as_deref()
        .is_none_or(|wire_selector| wire_selector_matches(wire_selector, provider))
}

fn plugin_model_info(model: &PluginLlmModel, slug: String, index: i32) -> ModelInfo {
    let mut info = known_openai_compatible_model_info(&slug)
        .unwrap_or_else(|| provider_neutral_plugin_model_info(slug.as_str(), index));
    info.slug = slug.clone();
    if let Some(display_name) = model
        .display_name
        .as_deref()
        .and_then(normalize_non_empty_string)
    {
        info.display_name = display_name;
    } else if info.display_name.trim().is_empty() {
        info.display_name = slug;
    }
    if let Some(description) = model
        .description
        .as_deref()
        .and_then(normalize_non_empty_string)
    {
        info.description = Some(description);
    }
    if let Some(priority) = model.priority {
        info.priority = priority;
    }
    if let Some(context_window) = model
        .context_window
        .filter(|context_window| *context_window > 0)
    {
        info.context_window = Some(context_window);
        info.auto_compact_token_limit = Some((context_window * 9) / 10);
    }
    if model.default_reasoning_effort.is_some() {
        info.default_reasoning_level = model.default_reasoning_effort;
    }
    info
}

fn provider_neutral_plugin_model_info(slug: &str, index: i32) -> ModelInfo {
    let (default_reasoning_level, supported_reasoning_levels) = provider_neutral_reasoning_levels();
    ModelInfo {
        slug: slug.to_string(),
        display_name: slug.to_string(),
        description: Some("Model metadata supplied by an enabled LLM plugin.".to_string()),
        default_reasoning_level,
        supported_reasoning_levels,
        shell_type: ConfigShellToolType::Default,
        visibility: ModelVisibility::List,
        supported_in_api: true,
        priority: 10_000 + index,
        availability_nux: None,
        upgrade: None,
        base_instructions: BASE_INSTRUCTIONS_DEFAULT.to_string(),
        model_messages: None,
        supports_reasoning_summaries: false,
        default_reasoning_summary: ReasoningSummary::Auto,
        support_verbosity: false,
        default_verbosity: None::<Verbosity>,
        apply_patch_tool_type: None::<ApplyPatchToolType>,
        web_search_tool_type: WebSearchToolType::Text,
        truncation_policy: TruncationPolicyConfig::bytes(/*limit*/ 10_000),
        supports_parallel_tool_calls: false,
        supports_image_detail_original: false,
        context_window: None,
        auto_compact_token_limit: None,
        effective_context_window_percent: 95,
        experimental_supported_tools: Vec::new(),
        input_modalities: default_input_modalities(),
        used_fallback_model_metadata: false,
        supports_search_tool: false,
    }
}

fn profile_id_matches(plugin_profile_id: &str, behavior_id: BehaviorProfileId) -> bool {
    let plugin_profile_id = normalize_profile_id(plugin_profile_id);
    behavior_profile_aliases(behavior_id)
        .iter()
        .any(|alias| plugin_profile_id == *alias)
}

fn behavior_profile_aliases(behavior_id: BehaviorProfileId) -> &'static [&'static str] {
    match behavior_id {
        BehaviorProfileId::CodexResponses => &["codex/responses", "codex/responses/base"],
        BehaviorProfileId::Common => &["common", "common/base"],
        BehaviorProfileId::DeepSeek => &["deepseek", "deepseek/base"],
        BehaviorProfileId::Gemini => &["gemini", "gemini/base"],
        BehaviorProfileId::Glm => &["glm", "glm/base"],
        BehaviorProfileId::Qwen => &["qwen", "qwen/base"],
        BehaviorProfileId::Claude => &["claude", "claude/base"],
        BehaviorProfileId::OpenRouter => &["openrouter", "openrouter/base"],
    }
}

fn product_id_matches(plugin_product: &PluginLlmProduct, product: ProductProfileId) -> bool {
    normalize_profile_id(&plugin_product.id) == product.as_str()
}

fn provider_selector_matches(
    provider_selector: &str,
    provider_id: &str,
    provider: &ModelProviderInfo,
) -> bool {
    selector_eq(provider_selector, provider_id) || selector_eq(provider_selector, &provider.name)
}

fn wire_selector_matches(wire_selector: &str, provider: &ModelProviderInfo) -> bool {
    let wire_selector = normalize_selector(wire_selector);
    let current_wire = normalize_selector(&provider.wire_api.to_string());
    wire_selector == current_wire || wire_selector == "common" && current_wire == "openai_compat"
}

fn resolve_prompt_slot<'a>(
    slots: impl Iterator<Item = &'a PluginLlmPromptSlot>,
    purpose: LlmPromptPurpose,
) -> Option<&'a PluginLlmPromptSlot> {
    slots.into_iter().find(|slot| {
        let slot = normalize_selector(&slot.slot);
        purpose
            .slots()
            .iter()
            .any(|candidate| slot == normalize_selector(candidate))
    })
}

fn join_optional_prompt_layers(base: Option<String>, product: Option<String>) -> Option<String> {
    match (base, product) {
        (Some(base), Some(product)) => Some(join_prompt_layers(&base, &product)),
        (Some(base), None) => Some(base),
        (None, Some(product)) => Some(product),
        (None, None) => None,
    }
}

fn join_prompt_layers(base: &str, product: &str) -> String {
    let base = base.trim();
    let product = product.trim();
    match (base.is_empty(), product.is_empty()) {
        (true, true) => String::new(),
        (true, false) => product.to_string(),
        (false, true) => base.to_string(),
        (false, false) => format!("{base}\n\n{product}"),
    }
}

fn read_plugin_prompt(
    slot: &PluginLlmPromptSlot,
    owner_id: &str,
    behavior_id: BehaviorProfileId,
    purpose: LlmPromptPurpose,
) -> Option<String> {
    let contents = match fs::read_to_string(slot.path.as_path()) {
        Ok(contents) => contents,
        Err(err) => {
            tracing::warn!(
                path = %slot.path.display(),
                plugin_llm_owner = owner_id,
                prompt_profile = behavior_id.as_str(),
                prompt_purpose = purpose.as_str(),
                "failed to read plugin LLM prompt: {err}"
            );
            return None;
        }
    };
    let prompt = contents.trim();
    if prompt.is_empty() {
        tracing::warn!(
            path = %slot.path.display(),
            plugin_llm_owner = owner_id,
            prompt_profile = behavior_id.as_str(),
            prompt_purpose = purpose.as_str(),
            "ignoring empty plugin LLM prompt"
        );
        return None;
    }

    tracing::debug!(
        path = %slot.path.display(),
        plugin_llm_owner = owner_id,
        prompt_profile = behavior_id.as_str(),
        prompt_purpose = purpose.as_str(),
        "resolved plugin LLM prompt"
    );
    Some(prompt.to_string())
}

fn read_tool_visibility_policy(
    path: &std::path::Path,
    behavior_id: BehaviorProfileId,
    policy_id: &str,
) -> Option<LlmToolVisibilityPolicy> {
    let contents = match fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(err) => {
            tracing::warn!(
                path = %path.display(),
                prompt_profile = behavior_id.as_str(),
                tool_policy = policy_id,
                "failed to read plugin LLM tool policy: {err}"
            );
            return None;
        }
    };
    let raw = match toml::from_str::<RawLlmToolVisibilityPolicy>(&contents) {
        Ok(raw) => raw,
        Err(err) => {
            tracing::warn!(
                path = %path.display(),
                prompt_profile = behavior_id.as_str(),
                tool_policy = policy_id,
                "failed to parse plugin LLM tool policy: {err}"
            );
            return None;
        }
    };

    let visible_tools = raw.visible_tools.map(|tools| {
        tools
            .into_iter()
            .filter_map(|tool| normalize_non_empty_tool_name(&tool))
            .collect::<HashSet<_>>()
    });
    let hidden_tools = raw
        .hidden_tools
        .into_iter()
        .filter_map(|tool| normalize_non_empty_tool_name(&tool))
        .collect::<HashSet<_>>();
    let policy = LlmToolVisibilityPolicy {
        visible_tools,
        hidden_tools,
    };

    (!policy.is_empty()).then_some(policy)
}

fn read_tool_capability_policy(
    path: &std::path::Path,
    behavior_id: BehaviorProfileId,
    policy_id: &str,
) -> Option<ToolCapabilityConfig> {
    let contents = match fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(err) => {
            tracing::warn!(
                path = %path.display(),
                prompt_profile = behavior_id.as_str(),
                tool_policy = policy_id,
                "failed to read plugin LLM tool capabilities: {err}"
            );
            return None;
        }
    };
    let raw = match toml::from_str::<RawLlmToolVisibilityPolicy>(&contents) {
        Ok(raw) => raw,
        Err(err) => {
            tracing::warn!(
                path = %path.display(),
                prompt_profile = behavior_id.as_str(),
                tool_policy = policy_id,
                "failed to parse plugin LLM tool capabilities: {err}"
            );
            return None;
        }
    };
    let capabilities = ToolCapabilityConfig {
        web_search_backend: raw.web_search_backend.map(|backend| match backend {
            RawLlmWebSearchBackend::Responses => ToolWebSearchBackend::Responses,
            RawLlmWebSearchBackend::Praxis => ToolWebSearchBackend::Praxis,
        }),
    };
    capabilities
        .web_search_backend
        .is_some()
        .then_some(capabilities)
}

fn merge_tool_capabilities(target: &mut ToolCapabilityConfig, source: ToolCapabilityConfig) {
    if source.web_search_backend.is_some() {
        target.web_search_backend = source.web_search_backend;
    }
}

fn read_task_policy(
    path: &std::path::Path,
    behavior_id: BehaviorProfileId,
) -> Option<LlmTaskPolicy> {
    let contents = match fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(err) => {
            tracing::warn!(
                path = %path.display(),
                prompt_profile = behavior_id.as_str(),
                "failed to read plugin LLM task policy: {err}"
            );
            return None;
        }
    };
    let raw = match toml::from_str::<RawLlmTaskPolicy>(&contents) {
        Ok(raw) => raw,
        Err(err) => {
            tracing::warn!(
                path = %path.display(),
                prompt_profile = behavior_id.as_str(),
                "failed to parse plugin LLM task policy: {err}"
            );
            return None;
        }
    };

    let auto_title = raw.auto_title.and_then(|policy| {
        let model_slug = policy
            .model
            .and_then(|model| normalize_non_empty_string(&model));
        let normalized = LlmAutoTitleTaskPolicy {
            model_slug,
            reasoning_effort: policy.reasoning_effort,
            suppress_model_default_reasoning: policy.suppress_model_default_reasoning,
        };
        (normalized.model_slug.is_some()
            || normalized.reasoning_effort.is_some()
            || normalized.suppress_model_default_reasoning.is_some())
        .then_some(normalized)
    });
    let (compact_execution, compact_model, auto_compact_token_limit_cap) = raw
        .compact
        .map(|policy| {
            (
                policy
                    .execution
                    .and_then(|execution| parse_compact_execution_policy(&execution)),
                policy
                    .model
                    .and_then(|model| normalize_non_empty_string(&model)),
                normalize_compact_token_limit_cap(policy.auto_compact_token_limit_cap),
            )
        })
        .unwrap_or_default();
    let policy = LlmTaskPolicy {
        auto_title,
        compact_execution,
        compact_model,
        auto_compact_token_limit_cap,
    };

    (!policy.is_empty()).then_some(policy)
}

fn parse_compact_execution_policy(value: &str) -> Option<CompactExecutionPolicy> {
    match normalize_selector(value).as_str() {
        "remote" | "remote_responses" | "responses" => {
            Some(CompactExecutionPolicy::RemoteResponses)
        }
        "local" | "local_prompt" | "prompt" => Some(CompactExecutionPolicy::LocalPrompt),
        _ => {
            tracing::warn!("ignoring unknown compact execution policy `{value}`");
            None
        }
    }
}

fn normalize_compact_token_limit_cap(value: Option<i64>) -> Option<i64> {
    match value {
        Some(value) if value > 0 => Some(value),
        Some(value) => {
            tracing::warn!("ignoring non-positive compact token limit cap `{value}`");
            None
        }
        None => None,
    }
}

fn selector_eq(left: &str, right: &str) -> bool {
    normalize_selector(left) == normalize_selector(right)
}

fn normalize_profile_id(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

fn normalize_selector(value: &str) -> String {
    let mut normalized = String::new();
    let mut previous_was_separator = false;
    let mut previous_was_lowercase = false;
    for ch in value.trim().chars() {
        if ch.is_ascii_uppercase() {
            if previous_was_lowercase && !previous_was_separator {
                normalized.push('_');
            }
            normalized.push(ch.to_ascii_lowercase());
            previous_was_separator = false;
            previous_was_lowercase = false;
        } else if ch.is_ascii_alphanumeric() {
            normalized.push(ch.to_ascii_lowercase());
            previous_was_separator = false;
            previous_was_lowercase = ch.is_ascii_lowercase() || ch.is_ascii_digit();
        } else if !previous_was_separator && !normalized.is_empty() {
            normalized.push('_');
            previous_was_separator = true;
            previous_was_lowercase = false;
        }
    }
    while normalized.ends_with('_') {
        normalized.pop();
    }
    normalized
}

fn normalize_non_empty_tool_name(value: &str) -> Option<String> {
    let value = normalize_tool_name(value);
    (!value.is_empty()).then_some(value)
}

fn normalize_non_empty_string(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_string())
}

fn normalize_tool_name(value: &str) -> String {
    value.trim().to_string()
}

trait ProductPromptBehaviorId {
    fn as_behavior_id(self) -> BehaviorProfileId;
}

impl ProductPromptBehaviorId for ProductProfileId {
    fn as_behavior_id(self) -> BehaviorProfileId {
        match self {
            ProductProfileId::Praxis | ProductProfileId::Cunning3d => BehaviorProfileId::Common,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model_provider_info::ModelProviderInfo;
    use crate::model_provider_info::WireApi;

    fn provider(id: &str, base_url: &str, wire_api: WireApi) -> (String, ModelProviderInfo) {
        (
            id.to_string(),
            ModelProviderInfo {
                name: id.to_string(),
                base_url: Some(base_url.to_string()),
                env_key: None,
                env_key_instructions: None,
                experimental_bearer_token: None,
                auth: None,
                wire_api,
                compat: None,
                query_params: None,
                http_headers: None,
                env_http_headers: None,
                request_max_retries: None,
                stream_max_retries: None,
                stream_idle_timeout_ms: None,
                websocket_connect_timeout_ms: None,
                requires_openai_auth: false,
                supports_websockets: false,
            },
        )
    }

    fn model(slug: &str) -> ModelInfo {
        crate::models_manager::model_info::model_info_from_slug(slug)
    }

    #[test]
    fn plugin_model_catalog_emits_provider_scoped_model_infos() {
        let catalog = LlmRuntimeCatalog::from_plugin_manifests(vec![PluginLlmManifest {
            profiles: Vec::new(),
            products: Vec::new(),
            tool_policies: Vec::new(),
            model_catalogs: vec![PluginLlmModelCatalog {
                id: "aliyun-coder".to_string(),
                label: Some("Aliyun Coder".to_string()),
                provider: Some("dashscope".to_string()),
                wire: Some("openai_compat".to_string()),
                models: vec![PluginLlmModel {
                    slug: "qwen3-coder-plus".to_string(),
                    display_name: Some("Qwen3 Coder Plus".to_string()),
                    description: Some("Aliyun coding model".to_string()),
                    priority: Some(20),
                    context_window: Some(262_144),
                    default_reasoning_effort: Some(ReasoningEffort::High),
                }],
            }],
        }]);
        let (provider_id, provider) = provider(
            "dashscope",
            "https://dashscope.aliyuncs.com/compatible-mode/v1",
            WireApi::OpenAiCompat,
        );

        let models = catalog.model_infos_for_provider(&provider_id, &provider);

        assert_eq!(models.len(), 1);
        assert_eq!(models[0].slug, "qwen3-coder-plus");
        assert_eq!(models[0].display_name, "Qwen3 Coder Plus");
        assert_eq!(models[0].priority, 20);
        assert_eq!(models[0].context_window, Some(262_144));
        assert_eq!(
            models[0].default_reasoning_level,
            Some(ReasoningEffort::High)
        );
    }

    #[test]
    fn plugin_profile_prompt_matches_alias_wire_and_camel_slot() {
        let temp_dir = tempfile::tempdir().unwrap();
        let prompt_path = temp_dir.path().join("title.md");
        std::fs::write(&prompt_path, "custom title prompt").unwrap();
        let catalog = LlmRuntimeCatalog::from_plugin_manifests(vec![PluginLlmManifest {
            profiles: vec![PluginLlmProfile {
                id: "deepseek/base".to_string(),
                provider: Some("deepseek".to_string()),
                wire: Some("common".to_string()),
                behavior: None,
                prompts: vec![PluginLlmPromptSlot {
                    slot: "autoTitle".to_string(),
                    path: praxis_utils_absolute_path::AbsolutePathBuf::try_from(prompt_path)
                        .unwrap(),
                }],
                tasks: None,
                tools: None,
            }],
            products: Vec::new(),
            tool_policies: Vec::new(),
            model_catalogs: Vec::new(),
        }]);
        let (provider_id, provider) = provider(
            "deepseek",
            "https://api.deepseek.com",
            WireApi::OpenAiCompat,
        );

        let prompt = catalog.resolve_profile_prompt_for_model(
            &model("deepseek-v4-pro"),
            &provider_id,
            &provider,
            LlmPromptPurpose::AutoTitle,
        );

        assert_eq!(prompt.as_deref(), Some("custom title prompt"));
    }

    #[test]
    fn tool_policies_match_behavior_aliases() {
        let temp_dir = tempfile::tempdir().unwrap();
        let policy_path = temp_dir.path().join("tools.toml");
        std::fs::write(
            &policy_path,
            "visible_tools = [\"web_search\"]\nhidden_tools = [\"shell\"]\n",
        )
        .unwrap();
        let catalog = LlmRuntimeCatalog::from_plugin_manifests(vec![PluginLlmManifest {
            profiles: Vec::new(),
            products: Vec::new(),
            tool_policies: vec![PluginLlmToolPolicy {
                id: "deepseek-tools".to_string(),
                path: praxis_utils_absolute_path::AbsolutePathBuf::try_from(policy_path).unwrap(),
                applies_to: vec!["deepseek/base".to_string()],
            }],
            model_catalogs: Vec::new(),
        }]);

        let policies = catalog.tool_policies_for_profile(BehaviorProfileId::DeepSeek);

        assert_eq!(policies.len(), 1);
        assert_eq!(policies[0].id, "deepseek-tools");
    }

    #[test]
    fn tool_visibility_policy_reads_matching_profile_policy() {
        let temp_dir = tempfile::tempdir().unwrap();
        let policy_path = temp_dir.path().join("tools.toml");
        std::fs::write(
            &policy_path,
            "visible_tools = [\"web_search\", \"view_image\"]\nhidden_tools = [\"shell_command\"]\n",
        )
        .unwrap();
        let catalog = LlmRuntimeCatalog::from_plugin_manifests(vec![PluginLlmManifest {
            profiles: vec![PluginLlmProfile {
                id: "deepseek".to_string(),
                provider: Some("deepseek".to_string()),
                wire: Some("common".to_string()),
                behavior: None,
                prompts: Vec::new(),
                tasks: None,
                tools: Some(
                    praxis_utils_absolute_path::AbsolutePathBuf::try_from(policy_path).unwrap(),
                ),
            }],
            products: Vec::new(),
            tool_policies: Vec::new(),
            model_catalogs: Vec::new(),
        }]);
        let (provider_id, provider) = provider(
            "deepseek",
            "https://api.deepseek.com",
            WireApi::OpenAiCompat,
        );

        let policy = catalog
            .tool_visibility_policy_for_model(
                &model("deepseek-v4-pro"),
                &provider_id,
                &provider,
                None,
            )
            .expect("tool policy");

        assert!(policy.allows("web_search"));
        assert!(policy.allows("view_image"));
        assert!(!policy.allows("shell_command"));
        assert!(!policy.allows("update_plan"));
    }

    #[test]
    fn task_policy_reads_matching_profile_policy() {
        let temp_dir = tempfile::tempdir().unwrap();
        let policy_path = temp_dir.path().join("tasks.toml");
        std::fs::write(
            &policy_path,
            "[auto_title]\nmodel = \"deepseek-v4-title\"\nreasoning_effort = \"low\"\nsuppress_model_default_reasoning = false\n\n[compact]\nexecution = \"local_prompt\"\nmodel = \"deepseek-v4-flash\"\nauto_compact_token_limit = 42000\n",
        )
        .unwrap();
        let catalog = LlmRuntimeCatalog::from_plugin_manifests(vec![PluginLlmManifest {
            profiles: vec![PluginLlmProfile {
                id: "deepseek".to_string(),
                provider: Some("deepseek".to_string()),
                wire: Some("common".to_string()),
                behavior: None,
                prompts: Vec::new(),
                tasks: Some(
                    praxis_utils_absolute_path::AbsolutePathBuf::try_from(policy_path).unwrap(),
                ),
                tools: None,
            }],
            products: Vec::new(),
            tool_policies: Vec::new(),
            model_catalogs: Vec::new(),
        }]);
        let (provider_id, provider) = provider(
            "deepseek",
            "https://api.deepseek.com",
            WireApi::OpenAiCompat,
        );

        let title_policy = catalog
            .auto_title_task_policy_for_model(
                &model("deepseek-v4-pro"),
                &provider_id,
                &provider,
                None,
            )
            .expect("auto-title task policy");
        let compact_policy = catalog.compact_execution_policy_for_model(
            &model("deepseek-v4-pro"),
            &provider_id,
            &provider,
            None,
        );
        let compact_model = catalog.compact_model_for_model(
            &model("deepseek-v4-pro"),
            &provider_id,
            &provider,
            None,
        );
        let compact_limit_cap = catalog.auto_compact_token_limit_cap_for_model(
            &model("deepseek-v4-pro"),
            &provider_id,
            &provider,
            None,
        );

        assert_eq!(
            title_policy.model_slug.as_deref(),
            Some("deepseek-v4-title")
        );
        assert_eq!(title_policy.reasoning_effort, Some(ReasoningEffort::Low));
        assert_eq!(title_policy.suppress_model_default_reasoning, Some(false));
        assert_eq!(compact_policy, Some(CompactExecutionPolicy::LocalPrompt));
        assert_eq!(compact_model.as_deref(), Some("deepseek-v4-flash"));
        assert_eq!(compact_limit_cap, Some(42_000));
    }

    #[test]
    fn product_prompt_and_tool_policy_layers_on_profile_policy() {
        let temp_dir = tempfile::tempdir().unwrap();
        let profile_prompt_path = temp_dir.path().join("profile.md");
        let product_prompt_path = temp_dir.path().join("product.md");
        let profile_tools_path = temp_dir.path().join("profile-tools.toml");
        let product_tools_path = temp_dir.path().join("product-tools.toml");
        std::fs::write(&profile_prompt_path, "profile prompt").unwrap();
        std::fs::write(&product_prompt_path, "product prompt").unwrap();
        std::fs::write(&profile_tools_path, "visible_tools = [\"web_search\"]\n").unwrap();
        std::fs::write(
            &product_tools_path,
            "visible_tools = [\"c3d_graph\"]\nhidden_tools = [\"shell_command\"]\n",
        )
        .unwrap();
        let catalog = LlmRuntimeCatalog::from_plugin_manifests(vec![PluginLlmManifest {
            profiles: vec![PluginLlmProfile {
                id: "deepseek".to_string(),
                provider: Some("deepseek".to_string()),
                wire: Some("common".to_string()),
                behavior: None,
                prompts: vec![PluginLlmPromptSlot {
                    slot: "autoTitle".to_string(),
                    path: praxis_utils_absolute_path::AbsolutePathBuf::try_from(
                        profile_prompt_path,
                    )
                    .unwrap(),
                }],
                tasks: None,
                tools: Some(
                    praxis_utils_absolute_path::AbsolutePathBuf::try_from(profile_tools_path)
                        .unwrap(),
                ),
            }],
            products: vec![PluginLlmProduct {
                id: "cunning3d".to_string(),
                prompts: vec![PluginLlmPromptSlot {
                    slot: "autoTitle".to_string(),
                    path: praxis_utils_absolute_path::AbsolutePathBuf::try_from(
                        product_prompt_path,
                    )
                    .unwrap(),
                }],
                tasks: None,
                tools: Some(
                    praxis_utils_absolute_path::AbsolutePathBuf::try_from(product_tools_path)
                        .unwrap(),
                ),
            }],
            tool_policies: Vec::new(),
            model_catalogs: Vec::new(),
        }]);
        let (provider_id, provider) = provider(
            "deepseek",
            "https://api.deepseek.com",
            WireApi::OpenAiCompat,
        );

        let prompt = catalog.resolve_prompt_for_model(
            &model("deepseek-v4-pro"),
            &provider_id,
            &provider,
            Some(ProductProfileId::Cunning3d),
            LlmPromptPurpose::AutoTitle,
        );
        let policy = catalog
            .tool_visibility_policy_for_model(
                &model("deepseek-v4-pro"),
                &provider_id,
                &provider,
                Some(ProductProfileId::Cunning3d),
            )
            .expect("tool policy");

        assert_eq!(prompt.as_deref(), Some("profile prompt\n\nproduct prompt"));
        assert!(policy.allows("web_search"));
        assert!(policy.allows("c3d_graph"));
        assert!(!policy.allows("shell_command"));
    }

    #[test]
    fn product_task_policy_overrides_profile_task_policy() {
        let temp_dir = tempfile::tempdir().unwrap();
        let profile_task_path = temp_dir.path().join("profile-tasks.toml");
        let product_task_path = temp_dir.path().join("product-tasks.toml");
        std::fs::write(
            &profile_task_path,
            "[auto_title]\nmodel = \"profile-title\"\nreasoning_effort = \"low\"\n\n[compact]\nexecution = \"local_prompt\"\nmodel = \"profile-compact\"\nauto_compact_token_limit = 42000\n",
        )
        .unwrap();
        std::fs::write(
            &product_task_path,
            "[auto_title]\nmodel = \"product-title\"\n\n[compact]\nexecution = \"remote_responses\"\ncompact_model = \"product-compact\"\nauto_compact_token_limit = 24000\n",
        )
        .unwrap();
        let catalog = LlmRuntimeCatalog::from_plugin_manifests(vec![PluginLlmManifest {
            profiles: vec![PluginLlmProfile {
                id: "deepseek".to_string(),
                provider: Some("deepseek".to_string()),
                wire: Some("common".to_string()),
                behavior: None,
                prompts: Vec::new(),
                tasks: Some(
                    praxis_utils_absolute_path::AbsolutePathBuf::try_from(profile_task_path)
                        .unwrap(),
                ),
                tools: None,
            }],
            products: vec![PluginLlmProduct {
                id: "cunning3d".to_string(),
                prompts: Vec::new(),
                tasks: Some(
                    praxis_utils_absolute_path::AbsolutePathBuf::try_from(product_task_path)
                        .unwrap(),
                ),
                tools: None,
            }],
            tool_policies: Vec::new(),
            model_catalogs: Vec::new(),
        }]);
        let (provider_id, provider) = provider(
            "deepseek",
            "https://api.deepseek.com",
            WireApi::OpenAiCompat,
        );

        let title_policy = catalog
            .auto_title_task_policy_for_model(
                &model("deepseek-v4-pro"),
                &provider_id,
                &provider,
                Some(ProductProfileId::Cunning3d),
            )
            .expect("auto-title task policy");
        let compact_policy = catalog.compact_execution_policy_for_model(
            &model("deepseek-v4-pro"),
            &provider_id,
            &provider,
            Some(ProductProfileId::Cunning3d),
        );
        let compact_model = catalog.compact_model_for_model(
            &model("deepseek-v4-pro"),
            &provider_id,
            &provider,
            Some(ProductProfileId::Cunning3d),
        );
        let compact_limit_cap = catalog.auto_compact_token_limit_cap_for_model(
            &model("deepseek-v4-pro"),
            &provider_id,
            &provider,
            Some(ProductProfileId::Cunning3d),
        );

        assert_eq!(title_policy.model_slug.as_deref(), Some("product-title"));
        assert_eq!(title_policy.reasoning_effort, Some(ReasoningEffort::Low));
        assert_eq!(
            compact_policy,
            Some(CompactExecutionPolicy::RemoteResponses)
        );
        assert_eq!(compact_model.as_deref(), Some("product-compact"));
        assert_eq!(compact_limit_cap, Some(24_000));
    }
}
