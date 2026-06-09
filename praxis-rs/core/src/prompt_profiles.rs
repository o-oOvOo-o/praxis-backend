use crate::llm::ids::BehaviorProfileId;
use crate::llm::ids::ProductProfileId;
use crate::llm::profiles::plugin::ProfileDescriptor;
use crate::llm::profiles::plugin::ProfileMatchContext;
use crate::llm::prompts::LlmPromptPurpose;
use crate::llm::registry::LlmProfileRegistry;
use crate::llm::runtime::LlmRuntimeCatalog;
use crate::model_provider_info::ModelProviderInfo;
use praxis_protocol::config_types::Personality;
use praxis_protocol::openai_models::ModelInfo;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PromptProfileId {
    CodexResponses,
    CommonBase,
    DeepSeek,
    Gemini,
    Glm,
    Qwen,
}

impl PromptProfileId {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::CodexResponses => "codex/responses",
            Self::CommonBase => "common/base",
            Self::DeepSeek => "deepseek/base",
            Self::Gemini => "gemini/base",
            Self::Glm => "glm/base",
            Self::Qwen => "qwen/base",
        }
    }

    fn from_behavior_id(profile_id: BehaviorProfileId) -> Option<Self> {
        match profile_id {
            BehaviorProfileId::CodexResponses => Some(Self::CodexResponses),
            BehaviorProfileId::Common => Some(Self::CommonBase),
            BehaviorProfileId::DeepSeek => Some(Self::DeepSeek),
            BehaviorProfileId::Gemini => Some(Self::Gemini),
            BehaviorProfileId::Glm => Some(Self::Glm),
            BehaviorProfileId::Qwen => Some(Self::Qwen),
            BehaviorProfileId::Claude | BehaviorProfileId::OpenRouter => None,
        }
    }
}

pub(crate) fn resolve_model_instructions(
    model_info: &ModelInfo,
    provider_id: &str,
    provider: &ModelProviderInfo,
    personality: Option<Personality>,
    product_profile: Option<ProductProfileId>,
    llm_runtime_catalog: &LlmRuntimeCatalog,
) -> String {
    let catalog_instructions = model_info.get_model_instructions(personality);
    let mut instructions =
        if let Some(profile) = resolve_prompt_profile(model_info, provider_id, provider) {
            resolve_behavior_model_instructions(
                model_info,
                provider_id,
                provider,
                profile,
                &catalog_instructions,
                llm_runtime_catalog,
            )
        } else {
            catalog_instructions
        };

    if let Some(product_instructions) = product_profile.and_then(|product| {
        llm_runtime_catalog.resolve_product_prompt(product, LlmPromptPurpose::ModelInstructions)
    }) {
        instructions = join_prompt_layers(&instructions, &product_instructions);
    }

    instructions
}

fn resolve_behavior_model_instructions(
    model_info: &ModelInfo,
    provider_id: &str,
    provider: &ModelProviderInfo,
    profile: ProfileDescriptor,
    catalog_instructions: &str,
    llm_runtime_catalog: &LlmRuntimeCatalog,
) -> String {
    if let Some(plugin_instructions) = llm_runtime_catalog.resolve_profile_prompt(
        profile,
        provider_id,
        provider,
        LlmPromptPurpose::ModelInstructions,
    ) {
        return plugin_instructions;
    }
    let instructions = match PromptProfileId::from_behavior_id(profile.id) {
        Some(profile_id)
            if profile_id == PromptProfileId::CodexResponses
                && !catalog_instructions.trim().is_empty() =>
        {
            catalog_instructions.to_string()
        }
        Some(profile_id) => {
            let profile_instructions = profile.instructions.unwrap_or_default().trim();
            if profile_instructions.is_empty() {
                catalog_instructions.to_string()
            } else {
                tracing::debug!(
                    model = %model_info.slug,
                    provider_id,
                    prompt_profile = profile_id.as_str(),
                    "resolved model prompt profile"
                );
                profile_instructions.to_string()
            }
        }
        None => catalog_instructions.to_string(),
    };

    join_profile_prompt_layers(instructions, profile, LlmPromptPurpose::ModelInstructions)
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

fn join_profile_prompt_layers(
    mut instructions: String,
    profile: ProfileDescriptor,
    purpose: LlmPromptPurpose,
) -> String {
    for layer in profile
        .prompt_layers
        .iter()
        .filter(|layer| layer.purpose == purpose)
    {
        let content = layer.content.trim();
        if content.is_empty() {
            continue;
        }
        tracing::debug!(
            prompt_profile = profile.id.as_str(),
            prompt_layer = layer.id,
            prompt_purpose = purpose.as_str(),
            "applied internal LLM prompt layer"
        );
        instructions = join_prompt_layers(&instructions, content);
    }
    instructions
}

#[cfg(test)]
pub(crate) fn infer_prompt_profile_id(
    model_info: &ModelInfo,
    provider_id: &str,
    provider: &ModelProviderInfo,
) -> Option<PromptProfileId> {
    resolve_prompt_profile(model_info, provider_id, provider)
        .and_then(|profile| PromptProfileId::from_behavior_id(profile.id))
}

fn resolve_prompt_profile(
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model_provider_info::ModelProviderInfo;
    use crate::model_provider_info::OPENAI_PROVIDER_ID;
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
    fn deepseek_provider_resolves_deepseek_profile() {
        let (provider_id, provider) = provider(
            "deepseek",
            "https://api.deepseek.com",
            WireApi::OpenAiCompat,
        );
        let profile = infer_prompt_profile_id(&model("deepseek-v4-pro"), &provider_id, &provider);
        assert_eq!(profile, Some(PromptProfileId::DeepSeek));
    }

    #[test]
    fn gemini_provider_resolves_gemini_profile() {
        let (provider_id, provider) = provider(
            "gemini",
            "https://generativelanguage.googleapis.com/v1beta/openai/chat/completions",
            WireApi::OpenAiCompat,
        );
        let profile =
            infer_prompt_profile_id(&model("gemini-3.1-pro-preview"), &provider_id, &provider);
        assert_eq!(profile, Some(PromptProfileId::Gemini));
    }

    #[test]
    fn unknown_openai_compatible_provider_resolves_common_profile() {
        let (provider_id, provider) = provider(
            "custom-provider",
            "https://example.test/v1",
            WireApi::OpenAiCompat,
        );
        let profile = infer_prompt_profile_id(&model("custom-model"), &provider_id, &provider);
        assert_eq!(profile, Some(PromptProfileId::CommonBase));
    }

    #[test]
    fn openai_gpt_model_resolves_codex_responses_profile() {
        let (provider_id, provider) = provider(
            OPENAI_PROVIDER_ID,
            "https://api.openai.com/v1",
            WireApi::Responses,
        );
        let profile = infer_prompt_profile_id(&model("gpt-5.2-codex"), &provider_id, &provider);
        assert_eq!(profile, Some(PromptProfileId::CodexResponses));
    }

    #[test]
    fn common_openai_compatible_profile_uses_common_instructions() {
        let (provider_id, provider) = provider(
            "custom-provider",
            "https://example.test/v1",
            WireApi::OpenAiCompat,
        );
        let model_info = model("custom-model");
        let instructions = resolve_model_instructions(
            &model_info,
            &provider_id,
            &provider,
            None,
            None,
            &LlmRuntimeCatalog::default(),
        );
        assert_eq!(instructions, crate::llm::profiles::common::prompts::BASE);
    }

    #[test]
    fn deepseek_profile_applies_internal_smarter_prompt_layer() {
        let (provider_id, provider) = provider(
            "deepseek",
            "https://api.deepseek.com",
            WireApi::OpenAiCompat,
        );

        let instructions = resolve_model_instructions(
            &model("deepseek-v4-pro"),
            &provider_id,
            &provider,
            None,
            None,
            &LlmRuntimeCatalog::default(),
        );

        assert!(instructions.starts_with("你是 Praxis"));
        assert!(instructions.contains("# DeepSeek Smarter Orchestration"));
        assert!(
            instructions.contains("优先把实现层委派给 Praxis 暴露的 Codex/OpenAI worker subagent")
        );
        assert!(instructions.contains("精确复制规则只适用于正向任务契约"));
        assert!(instructions.contains("不得把该字面量传给 worker、写入工具参数或放进最终答复"));
        assert!(
            instructions.contains("assign_task.constraints")
                && instructions.contains("assign_task.acceptance_criteria")
                && instructions.contains("assign_task.required_resources")
        );
        assert!(
            instructions.find("你是 Praxis").unwrap()
                < instructions
                    .find("# DeepSeek Smarter Orchestration")
                    .unwrap()
        );
    }

    #[test]
    fn deepseek_smarter_layer_does_not_apply_to_common_profile() {
        let (provider_id, provider) = provider(
            "custom-provider",
            "https://example.test/v1",
            WireApi::OpenAiCompat,
        );

        let instructions = resolve_model_instructions(
            &model("custom-model"),
            &provider_id,
            &provider,
            None,
            None,
            &LlmRuntimeCatalog::default(),
        );

        assert!(!instructions.contains("# DeepSeek Smarter Orchestration"));
    }

    #[test]
    fn plugin_profile_prompt_overrides_matching_model_instructions() {
        let temp_dir = tempfile::tempdir().unwrap();
        let prompt_path = temp_dir.path().join("deepseek-system.md");
        std::fs::write(&prompt_path, "\nplugin deepseek instructions\n").unwrap();
        let catalog =
            LlmRuntimeCatalog::from_plugin_manifests(vec![praxis_plugin::PluginLlmManifest {
                profiles: vec![praxis_plugin::PluginLlmProfile {
                    id: "deepseek/base".to_string(),
                    provider: Some("deepseek".to_string()),
                    wire: Some("common".to_string()),
                    behavior: None,
                    prompts: vec![praxis_plugin::PluginLlmPromptSlot {
                        slot: "base".to_string(),
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

        let instructions = resolve_model_instructions(
            &model("deepseek-v4-pro"),
            &provider_id,
            &provider,
            None,
            None,
            &catalog,
        );

        assert_eq!(instructions, "plugin deepseek instructions");
    }

    #[test]
    fn plugin_product_prompt_layers_over_behavior_prompt() {
        let temp_dir = tempfile::tempdir().unwrap();
        let profile_prompt_path = temp_dir.path().join("deepseek-system.md");
        let product_prompt_path = temp_dir.path().join("cunning3d-system.md");
        std::fs::write(&profile_prompt_path, "\nplugin deepseek instructions\n").unwrap();
        std::fs::write(&product_prompt_path, "\nplugin cunning3d instructions\n").unwrap();
        let catalog =
            LlmRuntimeCatalog::from_plugin_manifests(vec![praxis_plugin::PluginLlmManifest {
                profiles: vec![praxis_plugin::PluginLlmProfile {
                    id: "deepseek/base".to_string(),
                    provider: Some("deepseek".to_string()),
                    wire: Some("common".to_string()),
                    behavior: None,
                    prompts: vec![praxis_plugin::PluginLlmPromptSlot {
                        slot: "base".to_string(),
                        path: praxis_utils_absolute_path::AbsolutePathBuf::try_from(
                            profile_prompt_path,
                        )
                        .unwrap(),
                    }],
                    tasks: None,
                    tools: None,
                }],
                products: vec![praxis_plugin::PluginLlmProduct {
                    id: "cunning3d".to_string(),
                    prompts: vec![praxis_plugin::PluginLlmPromptSlot {
                        slot: "base".to_string(),
                        path: praxis_utils_absolute_path::AbsolutePathBuf::try_from(
                            product_prompt_path,
                        )
                        .unwrap(),
                    }],
                    tasks: None,
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

        let instructions = resolve_model_instructions(
            &model("deepseek-v4-pro"),
            &provider_id,
            &provider,
            None,
            Some(ProductProfileId::Cunning3d),
            &catalog,
        );

        assert_eq!(
            instructions,
            "plugin deepseek instructions\n\nplugin cunning3d instructions"
        );
    }
}
