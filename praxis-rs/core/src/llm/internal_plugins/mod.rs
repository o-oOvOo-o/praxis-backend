pub(crate) mod codex;
pub(crate) mod common;
pub(crate) mod common_branches;
pub(crate) mod deepseek;
pub(crate) mod glm;
pub(crate) mod qwen;
pub(crate) mod web_search;

use crate::llm::ids::BehaviorProfileId;
use crate::llm::ids::WireId;
use crate::llm::profiles::plugin::FirstPartyModelMatcher;
use crate::llm::profiles::plugin::FirstPartyProviderMatcher;
use crate::llm::profiles::plugin::ProfileDescriptor;
use crate::llm::wire::plugin::WireDescriptor;
use crate::model_provider_info::ModelProviderInfo;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum LlmExtensionKind {
    Wire,
    Provider,
    ModelCatalog,
    BehaviorProfile,
    PromptLayer,
    TaskPolicy,
    ToolPolicy,
    ToolCapability,
    ToolBackend,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LlmExtensionDescriptor {
    pub(crate) kind: LlmExtensionKind,
    pub(crate) id: &'static str,
    pub(crate) label: &'static str,
}

impl LlmExtensionDescriptor {
    pub(crate) const fn new(kind: LlmExtensionKind, id: &'static str, label: &'static str) -> Self {
        Self { kind, id, label }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LlmPluginDescriptor {
    pub(crate) id: &'static str,
    pub(crate) label: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LlmModelCatalogScope {
    Exclusive,
    ProviderExclusive,
    Generic,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct LlmModelCatalogDescriptor {
    pub(crate) id: &'static str,
    pub(crate) label: &'static str,
    pub(crate) scope: LlmModelCatalogScope,
    pub(crate) provider_matches: FirstPartyProviderMatcher,
    pub(crate) model_matches: FirstPartyModelMatcher,
}

impl LlmModelCatalogDescriptor {
    pub(crate) fn matches_provider(self, provider_id: &str, provider: &ModelProviderInfo) -> bool {
        (self.provider_matches)(provider_id, provider)
    }

    pub(crate) fn matches_model(self, model: &str) -> bool {
        (self.model_matches)(model)
    }
}

pub(crate) trait LlmPlugin {
    fn descriptor(&self) -> LlmPluginDescriptor;
    fn build(&self, registry: &mut LlmPluginRegistryBuilder);
}

#[derive(Debug, Default)]
pub(crate) struct LlmPluginRegistryBuilder {
    plugins: Vec<LlmPluginDescriptor>,
    extensions: Vec<LlmExtensionDescriptor>,
    model_catalogs: Vec<LlmModelCatalogDescriptor>,
    wires: Vec<WireDescriptor>,
    profiles: Vec<ProfileDescriptor>,
}

impl LlmPluginRegistryBuilder {
    pub(crate) fn add_plugin<P: LlmPlugin>(&mut self, plugin: P) {
        self.plugins.push(plugin.descriptor());
        plugin.build(self);
    }

    pub(crate) fn add_extension(&mut self, extension: LlmExtensionDescriptor) {
        if !self
            .extensions
            .iter()
            .any(|existing| existing.kind == extension.kind && existing.id == extension.id)
        {
            self.extensions.push(extension);
        }
    }

    pub(crate) fn add_model_catalog(&mut self, catalog: LlmModelCatalogDescriptor) {
        if !self
            .model_catalogs
            .iter()
            .any(|existing| existing.id == catalog.id)
        {
            self.model_catalogs.push(catalog);
        }
        self.add_extension(LlmExtensionDescriptor::new(
            LlmExtensionKind::ModelCatalog,
            catalog.id,
            catalog.label,
        ));
    }

    pub(crate) fn add_wire(&mut self, wire: WireDescriptor) {
        if !self.wires.iter().any(|existing| existing.id == wire.id) {
            self.wires.push(wire);
        }
    }

    pub(crate) fn add_profile(&mut self, profile: ProfileDescriptor) {
        if !self
            .profiles
            .iter()
            .any(|existing| existing.id == profile.id)
        {
            self.profiles.push(profile);
        }
    }

    fn build(self) -> LlmPluginRegistry {
        LlmPluginRegistry {
            plugins: self.plugins,
            extensions: self.extensions,
            model_catalogs: self.model_catalogs,
            wires: self.wires,
            profiles: self.profiles,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct LlmPluginRegistry {
    plugins: Vec<LlmPluginDescriptor>,
    extensions: Vec<LlmExtensionDescriptor>,
    model_catalogs: Vec<LlmModelCatalogDescriptor>,
    wires: Vec<WireDescriptor>,
    profiles: Vec<ProfileDescriptor>,
}

impl LlmPluginRegistry {
    pub(crate) fn plugins(&self) -> &[LlmPluginDescriptor] {
        &self.plugins
    }

    pub(crate) fn extensions(&self) -> &[LlmExtensionDescriptor] {
        &self.extensions
    }

    pub(crate) fn model_catalogs(&self) -> &[LlmModelCatalogDescriptor] {
        &self.model_catalogs
    }

    pub(crate) fn wires(&self) -> &[WireDescriptor] {
        &self.wires
    }

    pub(crate) fn profiles(&self) -> &[ProfileDescriptor] {
        &self.profiles
    }
}

pub(crate) fn exclusive_model_catalog(
    id: &'static str,
    label: &'static str,
    provider_matches: FirstPartyProviderMatcher,
    model_matches: FirstPartyModelMatcher,
) -> LlmModelCatalogDescriptor {
    LlmModelCatalogDescriptor {
        id,
        label,
        scope: LlmModelCatalogScope::Exclusive,
        provider_matches,
        model_matches,
    }
}

pub(crate) fn provider_model_catalog(
    id: &'static str,
    label: &'static str,
    provider_matches: FirstPartyProviderMatcher,
    model_matches: FirstPartyModelMatcher,
) -> LlmModelCatalogDescriptor {
    LlmModelCatalogDescriptor {
        id,
        label,
        scope: LlmModelCatalogScope::ProviderExclusive,
        provider_matches,
        model_matches,
    }
}

pub(crate) fn generic_model_catalog(
    id: &'static str,
    label: &'static str,
    provider_matches: FirstPartyProviderMatcher,
    model_matches: FirstPartyModelMatcher,
) -> LlmModelCatalogDescriptor {
    LlmModelCatalogDescriptor {
        id,
        label,
        scope: LlmModelCatalogScope::Generic,
        provider_matches,
        model_matches,
    }
}

pub(crate) fn builtin_registry() -> LlmPluginRegistry {
    let mut builder = LlmPluginRegistryBuilder::default();
    builder.add_plugin(web_search::WebSearchLlmPlugin);
    builder.add_plugin(codex::CodexLlmPlugin);
    builder.add_plugin(common::CommonOpenAiCompatLlmPlugin);
    builder.add_plugin(deepseek::DeepSeekLlmPlugin);
    builder.add_plugin(glm::GlmLlmPlugin);
    builder.add_plugin(qwen::QwenLlmPlugin);
    builder.build()
}

pub(crate) fn behavior_extension(
    profile: BehaviorProfileId,
    label: &'static str,
) -> LlmExtensionDescriptor {
    LlmExtensionDescriptor::new(LlmExtensionKind::BehaviorProfile, profile.as_str(), label)
}

pub(crate) fn wire_extension(wire: WireId, label: &'static str) -> LlmExtensionDescriptor {
    LlmExtensionDescriptor::new(LlmExtensionKind::Wire, wire.as_str(), label)
}

pub(crate) fn tool_capability_extension(
    capability: &'static str,
    label: &'static str,
) -> LlmExtensionDescriptor {
    LlmExtensionDescriptor::new(LlmExtensionKind::ToolCapability, capability, label)
}

pub(crate) fn tool_backend_extension(
    backend: &'static str,
    label: &'static str,
) -> LlmExtensionDescriptor {
    LlmExtensionDescriptor::new(LlmExtensionKind::ToolBackend, backend, label)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_registry_exposes_large_provider_plugins() {
        let registry = builtin_registry();

        assert_eq!(
            registry
                .plugins()
                .iter()
                .map(|plugin| plugin.id)
                .collect::<Vec<_>>(),
            vec![
                "web_search",
                "codex",
                "openai_compat",
                "deepseek",
                "glm",
                "qwen"
            ]
        );
        assert!(
            registry
                .profiles()
                .iter()
                .any(|profile| profile.id == BehaviorProfileId::CodexResponses)
        );
        assert!(
            registry
                .profiles()
                .iter()
                .any(|profile| profile.id == BehaviorProfileId::DeepSeek)
        );
        assert!(
            registry
                .profiles()
                .iter()
                .any(|profile| profile.id == BehaviorProfileId::Common)
        );
        assert!(
            registry
                .profiles()
                .iter()
                .any(|profile| profile.id == BehaviorProfileId::Qwen)
        );
        assert!(
            registry
                .profiles()
                .iter()
                .any(|profile| profile.id == BehaviorProfileId::Glm)
        );
        assert!(
            registry
                .profiles()
                .iter()
                .any(|profile| profile.id == BehaviorProfileId::OpenRouter)
        );
        assert!(
            !registry
                .profiles()
                .iter()
                .any(|profile| profile.id == BehaviorProfileId::Claude)
        );
        assert!(
            registry
                .wires()
                .iter()
                .any(|wire| wire.id == WireId::Responses)
        );
        assert!(
            registry
                .wires()
                .iter()
                .any(|wire| wire.id == WireId::OpenAiCompat)
        );
    }

    #[test]
    fn codex_plugin_registers_codex_specific_extension_bundle() {
        let registry = builtin_registry();
        let codex_extension_ids = registry
            .extensions()
            .iter()
            .filter(|extension| extension.id.starts_with("codex"))
            .map(|extension| extension.kind)
            .collect::<std::collections::HashSet<_>>();

        assert!(codex_extension_ids.contains(&LlmExtensionKind::BehaviorProfile));
        assert!(codex_extension_ids.contains(&LlmExtensionKind::PromptLayer));
        assert!(codex_extension_ids.contains(&LlmExtensionKind::TaskPolicy));
        assert!(codex_extension_ids.contains(&LlmExtensionKind::ToolPolicy));
    }

    #[test]
    fn praxis_web_search_is_runtime_backend_for_non_responses_profiles() {
        let registry = builtin_registry();
        let extension_ids = registry
            .extensions()
            .iter()
            .map(|extension| extension.id)
            .collect::<std::collections::HashSet<_>>();

        assert!(extension_ids.contains("web_search"));
        assert!(extension_ids.contains("openai_compat/model_catalog"));
        assert!(extension_ids.contains("web_search/praxis"));
        assert!(extension_ids.contains("web_search/responses"));
        assert!(extension_ids.contains("common/openrouter"));
        assert!(extension_ids.contains("common/claude_placeholder"));
        assert!(extension_ids.contains("common/claude_placeholder/statement"));
        assert!(!extension_ids.contains("praxis/web_search"));
        assert!(!extension_ids.contains("praxis/web_search/obscura"));
        assert!(!extension_ids.contains("deepseek/web_search"));
        assert!(!extension_ids.contains("deepseek/web_search/local_function"));
        assert!(!extension_ids.contains("codex/web_search/responses_native"));
        assert!(!extension_ids.contains("claude/prompts"));
    }

    #[test]
    fn provider_model_visibility_comes_from_registered_catalogs() {
        let registry = builtin_registry();
        let openai = ModelProviderInfo::create_openai_provider(None);
        let deepseek = ModelProviderInfo {
            name: "deepseek".to_string(),
            base_url: Some("https://api.deepseek.com".to_string()),
            env_key: None,
            env_key_instructions: None,
            experimental_bearer_token: None,
            auth: None,
            wire_api: crate::model_provider_info::WireApi::OpenAiCompat,
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
        };
        let glm = ModelProviderInfo {
            name: "GLM".to_string(),
            base_url: Some(
                "https://token-plan.cn-beijing.maas.aliyuncs.com/compatible-mode/v1".to_string(),
            ),
            env_key: None,
            env_key_instructions: None,
            experimental_bearer_token: None,
            auth: None,
            wire_api: crate::model_provider_info::WireApi::OpenAiCompat,
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
        };
        let qwen = ModelProviderInfo {
            name: "Qwen".to_string(),
            base_url: Some("https://dashscope.aliyuncs.com/compatible-mode/v1".to_string()),
            env_key: None,
            env_key_instructions: None,
            experimental_bearer_token: None,
            auth: None,
            wire_api: crate::model_provider_info::WireApi::OpenAiCompat,
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
        };
        let custom = ModelProviderInfo {
            name: "custom".to_string(),
            base_url: Some("https://example.test/v1".to_string()),
            env_key: None,
            env_key_instructions: None,
            experimental_bearer_token: None,
            auth: None,
            wire_api: crate::model_provider_info::WireApi::OpenAiCompat,
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
        };

        assert!(crate::llm::registry::provider_accepts_model_from_catalogs(
            registry.model_catalogs(),
            "openai",
            &openai,
            "gpt-5.2-codex"
        ));
        assert!(!crate::llm::registry::provider_accepts_model_from_catalogs(
            registry.model_catalogs(),
            "openai",
            &openai,
            "deepseek-v4-pro"
        ));
        assert!(crate::llm::registry::provider_accepts_model_from_catalogs(
            registry.model_catalogs(),
            "deepseek",
            &deepseek,
            "deepseek-v4-pro"
        ));
        assert!(!crate::llm::registry::provider_accepts_model_from_catalogs(
            registry.model_catalogs(),
            "deepseek",
            &deepseek,
            "gpt-5.2-codex"
        ));
        assert!(crate::llm::registry::provider_accepts_model_from_catalogs(
            registry.model_catalogs(),
            "glm",
            &glm,
            "glm-5.1"
        ));
        assert!(!crate::llm::registry::provider_accepts_model_from_catalogs(
            registry.model_catalogs(),
            "glm",
            &glm,
            "qwen3.7-max"
        ));
        assert!(crate::llm::registry::provider_accepts_model_from_catalogs(
            registry.model_catalogs(),
            "qwen",
            &qwen,
            "qwen3.7-max"
        ));
        assert!(crate::llm::registry::provider_accepts_model_from_catalogs(
            registry.model_catalogs(),
            "qwen",
            &qwen,
            "qwen3-coder"
        ));
        assert!(!crate::llm::registry::provider_accepts_model_from_catalogs(
            registry.model_catalogs(),
            "qwen",
            &qwen,
            "glm-5.1"
        ));
        assert!(!crate::llm::registry::provider_accepts_model_from_catalogs(
            registry.model_catalogs(),
            "qwen",
            &qwen,
            "qwen-image-2.0"
        ));
        assert!(crate::llm::registry::provider_accepts_model_from_catalogs(
            registry.model_catalogs(),
            "custom",
            &custom,
            "glm-5.1"
        ));
        assert!(crate::llm::registry::provider_accepts_model_from_catalogs(
            registry.model_catalogs(),
            "custom",
            &custom,
            "qwen3-coder"
        ));
    }

    #[test]
    fn common_openai_compat_profile_reuses_deepseek_prompt_for_now() {
        let registry = builtin_registry();
        let common = registry
            .profiles()
            .iter()
            .find(|profile| profile.id == BehaviorProfileId::Common)
            .expect("common openai-compatible profile");

        assert_eq!(
            common.instructions,
            Some(crate::llm::profiles::deepseek::prompts::BASE)
        );
    }
}
