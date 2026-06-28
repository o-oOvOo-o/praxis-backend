mod common;
mod common_branches;
mod deepseek;
mod gemini;
mod glm;
mod openai_responses;
mod qwen;
#[cfg(test)]
mod web_search;

#[cfg(test)]
use crate::llm::ids::BehaviorProfileId;
#[cfg(test)]
use crate::llm::ids::WireId;
use crate::llm::profiles::plugin::FirstPartyModelMatcher;
use crate::llm::profiles::plugin::FirstPartyProviderMatcher;
use crate::llm::profiles::plugin::ProfileDescriptor;
use crate::model_provider_info::ModelProviderInfo;

#[cfg(test)]
const OPENAI_COMPAT_WIRE_LABEL: &str = "OpenAI-compatible Chat Completions";

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum LlmExtensionKind {
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

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct LlmExtensionDescriptor {
    kind: LlmExtensionKind,
    id: &'static str,
    label: &'static str,
}

#[cfg(test)]
impl LlmExtensionDescriptor {
    const fn new(kind: LlmExtensionKind, id: &'static str, label: &'static str) -> Self {
        Self { kind, id, label }
    }
}

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct LlmPluginDescriptor {
    id: &'static str,
    label: &'static str,
}

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct WireDescriptor {
    id: WireId,
    name: &'static str,
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
    #[cfg(test)]
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

trait LlmPlugin {
    #[cfg(test)]
    fn descriptor(&self) -> LlmPluginDescriptor;
    fn build(&self, registry: &mut LlmPluginRegistryBuilder);
}

#[derive(Debug, Default)]
struct LlmPluginRegistryBuilder {
    #[cfg(test)]
    plugins: Vec<LlmPluginDescriptor>,
    #[cfg(test)]
    extensions: Vec<LlmExtensionDescriptor>,
    model_catalogs: Vec<LlmModelCatalogDescriptor>,
    #[cfg(test)]
    wires: Vec<WireDescriptor>,
    profiles: Vec<ProfileDescriptor>,
}

impl LlmPluginRegistryBuilder {
    fn add_plugin<P: LlmPlugin>(&mut self, plugin: P) {
        #[cfg(test)]
        self.plugins.push(plugin.descriptor());
        plugin.build(self);
    }

    #[cfg(test)]
    fn add_extension(&mut self, extension: LlmExtensionDescriptor) {
        if !self
            .extensions
            .iter()
            .any(|existing| existing.kind == extension.kind && existing.id == extension.id)
        {
            self.extensions.push(extension);
        }
    }

    #[cfg(test)]
    fn add_provider_extension(&mut self, id: &'static str, label: &'static str) {
        #[cfg(test)]
        self.add_extension(LlmExtensionDescriptor::new(
            LlmExtensionKind::Provider,
            id,
            label,
        ));
    }

    #[cfg(test)]
    fn add_profile_provider_extension(&mut self, profile: ProfileDescriptor, label: &'static str) {
        let id = profile
            .provider_policy
            .and_then(|policy| policy.canonical_provider_id())
            .expect("first-party profile must declare a canonical provider id");
        self.add_provider_extension(id, label);
    }

    #[cfg(test)]
    fn add_prompt_layer_extension(&mut self, id: &'static str, label: &'static str) {
        #[cfg(test)]
        self.add_extension(LlmExtensionDescriptor::new(
            LlmExtensionKind::PromptLayer,
            id,
            label,
        ));
    }

    #[cfg(test)]
    fn add_task_policy_extension(&mut self, id: &'static str, label: &'static str) {
        #[cfg(test)]
        self.add_extension(LlmExtensionDescriptor::new(
            LlmExtensionKind::TaskPolicy,
            id,
            label,
        ));
    }

    #[cfg(test)]
    fn add_tool_policy_extension(&mut self, id: &'static str, label: &'static str) {
        #[cfg(test)]
        self.add_extension(LlmExtensionDescriptor::new(
            LlmExtensionKind::ToolPolicy,
            id,
            label,
        ));
    }

    #[cfg(test)]
    fn add_tool_capability_extension(&mut self, id: &'static str, label: &'static str) {
        #[cfg(test)]
        self.add_extension(LlmExtensionDescriptor::new(
            LlmExtensionKind::ToolCapability,
            id,
            label,
        ));
    }

    #[cfg(test)]
    fn add_tool_backend_extension(&mut self, id: &'static str, label: &'static str) {
        #[cfg(test)]
        self.add_extension(LlmExtensionDescriptor::new(
            LlmExtensionKind::ToolBackend,
            id,
            label,
        ));
    }

    #[cfg(test)]
    fn add_profile_extension_bundle(
        &mut self,
        profile: ProfileDescriptor,
        prompt: (&'static str, &'static str),
        task: (&'static str, &'static str),
        tool: (&'static str, &'static str),
    ) {
        #[cfg(test)]
        self.add_extension(LlmExtensionDescriptor::new(
            LlmExtensionKind::BehaviorProfile,
            profile.id.as_str(),
            profile.label,
        ));
        self.add_prompt_layer_extension(prompt.0, prompt.1);
        self.add_task_policy_extension(task.0, task.1);
        self.add_tool_policy_extension(tool.0, tool.1);
    }

    fn add_model_catalog(&mut self, catalog: LlmModelCatalogDescriptor) {
        if !self
            .model_catalogs
            .iter()
            .any(|existing| existing.id == catalog.id)
        {
            self.model_catalogs.push(catalog);
        }
        #[cfg(test)]
        self.add_extension(LlmExtensionDescriptor::new(
            LlmExtensionKind::ModelCatalog,
            catalog.id,
            catalog.label,
        ));
    }

    #[cfg(test)]
    fn add_wire_descriptor(&mut self, id: WireId, name: &'static str) {
        #[cfg(test)]
        {
            if !self.wires.iter().any(|existing| existing.id == id) {
                self.wires.push(WireDescriptor { id, name });
            }
            self.add_extension(LlmExtensionDescriptor::new(
                LlmExtensionKind::Wire,
                id.as_str(),
                name,
            ));
        }
    }

    #[cfg(test)]
    fn add_openai_compat_wire(&mut self) {
        self.add_wire_descriptor(WireId::OpenAiCompat, OPENAI_COMPAT_WIRE_LABEL);
    }

    fn add_profile(&mut self, profile: ProfileDescriptor) {
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
            #[cfg(test)]
            plugins: self.plugins,
            #[cfg(test)]
            extensions: self.extensions,
            model_catalogs: self.model_catalogs,
            #[cfg(test)]
            wires: self.wires,
            profiles: self.profiles,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct LlmPluginRegistry {
    #[cfg(test)]
    plugins: Vec<LlmPluginDescriptor>,
    #[cfg(test)]
    extensions: Vec<LlmExtensionDescriptor>,
    model_catalogs: Vec<LlmModelCatalogDescriptor>,
    #[cfg(test)]
    wires: Vec<WireDescriptor>,
    profiles: Vec<ProfileDescriptor>,
}

impl LlmPluginRegistry {
    #[cfg(test)]
    fn plugins(&self) -> &[LlmPluginDescriptor] {
        &self.plugins
    }

    #[cfg(test)]
    fn extensions(&self) -> &[LlmExtensionDescriptor] {
        &self.extensions
    }

    pub(crate) fn model_catalogs(&self) -> &[LlmModelCatalogDescriptor] {
        &self.model_catalogs
    }

    #[cfg(test)]
    fn wires(&self) -> &[WireDescriptor] {
        &self.wires
    }

    pub(crate) fn profiles(&self) -> &[ProfileDescriptor] {
        &self.profiles
    }
}

fn exclusive_model_catalog(
    id: &'static str,
    _label: &'static str,
    provider_matches: FirstPartyProviderMatcher,
    model_matches: FirstPartyModelMatcher,
) -> LlmModelCatalogDescriptor {
    LlmModelCatalogDescriptor {
        id,
        #[cfg(test)]
        label: _label,
        scope: LlmModelCatalogScope::Exclusive,
        provider_matches,
        model_matches,
    }
}

fn provider_model_catalog(
    id: &'static str,
    _label: &'static str,
    provider_matches: FirstPartyProviderMatcher,
    model_matches: FirstPartyModelMatcher,
) -> LlmModelCatalogDescriptor {
    LlmModelCatalogDescriptor {
        id,
        #[cfg(test)]
        label: _label,
        scope: LlmModelCatalogScope::ProviderExclusive,
        provider_matches,
        model_matches,
    }
}

fn generic_model_catalog(
    id: &'static str,
    _label: &'static str,
    provider_matches: FirstPartyProviderMatcher,
    model_matches: FirstPartyModelMatcher,
) -> LlmModelCatalogDescriptor {
    LlmModelCatalogDescriptor {
        id,
        #[cfg(test)]
        label: _label,
        scope: LlmModelCatalogScope::Generic,
        provider_matches,
        model_matches,
    }
}

pub(crate) fn builtin_registry() -> LlmPluginRegistry {
    let mut builder = LlmPluginRegistryBuilder::default();
    #[cfg(test)]
    builder.add_plugin(web_search::WebSearchLlmPlugin);
    builder.add_plugin(openai_responses::OpenAiResponsesLlmPlugin);
    builder.add_plugin(common::CommonOpenAiCompatLlmPlugin);
    builder.add_plugin(deepseek::DeepSeekLlmPlugin);
    builder.add_plugin(gemini::GeminiLlmPlugin);
    builder.add_plugin(glm::GlmLlmPlugin);
    builder.add_plugin(qwen::QwenLlmPlugin);
    builder.build()
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
                "openai_responses",
                "openai_compat",
                "deepseek",
                "gemini",
                "glm",
                "qwen"
            ]
        );
        assert!(
            registry
                .profiles()
                .iter()
                .any(|profile| profile.id == BehaviorProfileId::OpenAiResponses)
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
                .any(|profile| profile.id == BehaviorProfileId::Gemini)
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
            registry
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
    fn openai_responses_plugin_registers_profile_extension_bundle() {
        let registry = builtin_registry();
        let openai_responses_extension_ids = registry
            .extensions()
            .iter()
            .filter(|extension| extension.id.starts_with("openai_responses"))
            .map(|extension| extension.kind)
            .collect::<std::collections::HashSet<_>>();

        assert!(openai_responses_extension_ids.contains(&LlmExtensionKind::PromptLayer));
        assert!(openai_responses_extension_ids.contains(&LlmExtensionKind::TaskPolicy));
        assert!(openai_responses_extension_ids.contains(&LlmExtensionKind::ToolPolicy));
        assert!(registry.extensions().iter().any(|extension| extension.kind
            == LlmExtensionKind::BehaviorProfile
            && extension.id == BehaviorProfileId::OpenAiResponses.as_str()));
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
        assert!(extension_ids.contains("common/claude"));
        assert!(extension_ids.contains("common/claude/prompts"));
        assert!(extension_ids.contains("common/claude/tasks"));
        assert!(extension_ids.contains("common/claude/tools"));
        assert!(extension_ids.contains("deepseek/smarter"));
        assert!(!extension_ids.contains("praxis/web_search"));
        assert!(!extension_ids.contains("praxis/web_search/obscura"));
        assert!(!extension_ids.contains("deepseek/web_search"));
        assert!(!extension_ids.contains("deepseek/web_search/local_function"));
        assert!(!extension_ids.contains("codex/web_search/responses_native"));
        assert!(!extension_ids.contains("common/claude_placeholder"));
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
    fn common_openai_compat_profile_uses_common_prompt() {
        let registry = builtin_registry();
        let common = registry
            .profiles()
            .iter()
            .find(|profile| profile.id == BehaviorProfileId::Common)
            .expect("common openai-compatible profile");

        assert_eq!(
            common.instructions,
            crate::llm::profiles::common::profile().instructions
        );
    }
}
