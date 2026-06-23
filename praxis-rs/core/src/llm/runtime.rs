use praxis_plugin::PluginLlmManifest;
#[cfg(test)]
use praxis_plugin::PluginLlmModel;
#[cfg(test)]
use praxis_plugin::PluginLlmModelCatalog;
#[cfg(test)]
use praxis_plugin::PluginLlmProduct;
#[cfg(test)]
use praxis_plugin::PluginLlmProfile;
#[cfg(test)]
use praxis_plugin::PluginLlmPromptSlot;
use praxis_plugin::PluginLlmToolPolicy;

use crate::config::Config;
use crate::llm::ids::BehaviorProfileId;
use crate::llm::ids::ProductProfileId;
use crate::llm::local_models::LocalModelHostRegistry;
use crate::llm::profiles::plugin::ProfileDescriptor;
use crate::llm::profiles::plugin::ProfileMatchContext;
use crate::llm::prompts::LlmPromptPurpose;
use crate::llm::registry::LlmProfileRegistry;
use crate::llm::tasks::compact::CompactExecutionPolicy;
use crate::llm::transcription::TranscriptionRuntime;
use crate::model_provider_info::ModelProviderInfo;
use praxis_protocol::openai_models::ModelInfo;
#[cfg(test)]
use praxis_protocol::openai_models::ReasoningEffort;
use praxis_tools::ToolCapabilityConfig;

mod manifest_lookup;
mod matching;
mod model_catalog;
mod normalization;
mod policies;
mod prompts;

pub(crate) use policies::LlmAutoTitleTaskPolicy;
use policies::LlmTaskPolicy;
pub(crate) use policies::LlmToolVisibilityPolicy;
use policies::merge_tool_capabilities;
use policies::read_task_policy;
use policies::read_tool_capability_policy;
use policies::read_tool_visibility_policy;
use prompts::join_optional_prompt_layers;

#[derive(Debug, Clone, Default)]
pub(crate) struct LlmRuntimeCatalog {
    plugin_manifests: Vec<PluginLlmManifest>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct LlmRuntimeSubstrate {
    pub(crate) local_model_hosts: LocalModelHostRegistry,
    pub(crate) transcription: TranscriptionRuntime,
}

impl LlmRuntimeSubstrate {
    pub(crate) fn from_config(config: &Config) -> Self {
        Self {
            local_model_hosts: LocalModelHostRegistry::new(config.local_model_hosts.clone()),
            transcription: TranscriptionRuntime::new(config.transcription.clone()),
        }
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
        model_catalog::model_infos_for_provider(&self.plugin_manifests, provider_id, provider)
    }

    pub(crate) fn merge_model_catalog_into_config(&self, config: &mut Config) {
        model_catalog::merge_model_catalog_into_config(&self.plugin_manifests, config);
    }

    pub(crate) fn resolve_builtin_profile(
        &self,
        model_info: &ModelInfo,
        provider_id: &str,
        provider: &ModelProviderInfo,
    ) -> Option<ProfileDescriptor> {
        let ctx = ProfileMatchContext::new(model_info, provider_id, provider);
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
        manifest_lookup::resolve_profile_prompt(
            &self.plugin_manifests,
            profile,
            provider_id,
            provider,
            purpose,
        )
    }

    pub(crate) fn resolve_product_prompt(
        &self,
        product: ProductProfileId,
        purpose: LlmPromptPurpose,
    ) -> Option<String> {
        manifest_lookup::resolve_product_prompt(&self.plugin_manifests, product, purpose)
    }

    pub(crate) fn profile_task_policy_path(
        &self,
        profile: ProfileDescriptor,
        provider_id: &str,
        provider: &ModelProviderInfo,
    ) -> Option<praxis_utils_absolute_path::AbsolutePathBuf> {
        manifest_lookup::profile_task_policy_path(
            &self.plugin_manifests,
            profile,
            provider_id,
            provider,
        )
    }

    pub(crate) fn profile_tools_policy_path(
        &self,
        profile: ProfileDescriptor,
        provider_id: &str,
        provider: &ModelProviderInfo,
    ) -> Option<praxis_utils_absolute_path::AbsolutePathBuf> {
        manifest_lookup::profile_tools_policy_path(
            &self.plugin_manifests,
            profile,
            provider_id,
            provider,
        )
    }

    pub(crate) fn product_task_policy_path(
        &self,
        product: ProductProfileId,
    ) -> Option<praxis_utils_absolute_path::AbsolutePathBuf> {
        manifest_lookup::product_task_policy_path(&self.plugin_manifests, product)
    }

    pub(crate) fn product_tools_policy_path(
        &self,
        product: ProductProfileId,
    ) -> Option<praxis_utils_absolute_path::AbsolutePathBuf> {
        manifest_lookup::product_tools_policy_path(&self.plugin_manifests, product)
    }

    pub(crate) fn tool_policies_for_profile(
        &self,
        profile: BehaviorProfileId,
    ) -> Vec<PluginLlmToolPolicy> {
        manifest_lookup::tool_policies_for_profile(&self.plugin_manifests, profile)
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
                product.policy_reader_behavior_id(),
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
                product.policy_reader_behavior_id(),
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
            && let Some(product_policy) =
                read_task_policy(path.as_path(), product.policy_reader_behavior_id())
        {
            policy.merge(product_policy);
        }
        (!policy.is_empty()).then_some(policy)
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
