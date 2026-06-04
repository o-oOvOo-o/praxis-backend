use crate::model_provider_info::ModelProviderInfo;
use crate::model_provider_info::OPENAI_PROVIDER_ID;
use crate::model_provider_info::WireApi;
use praxis_protocol::config_types::Personality;
use praxis_protocol::openai_models::ModelInfo;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PromptProfileId {
    CodexResponses,
    CommonBase,
    DeepSeek,
    Glm,
    Qwen,
}

impl PromptProfileId {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::CodexResponses => "codex/responses",
            Self::CommonBase => "common/base",
            Self::DeepSeek => "deepseek/base",
            Self::Glm => "glm/base",
            Self::Qwen => "qwen/base",
        }
    }

    fn instructions(self) -> &'static str {
        match self {
            Self::CodexResponses => {
                include_str!("../templates/prompt_profiles/codex/responses.md")
            }
            Self::CommonBase => include_str!("../templates/prompt_profiles/common/base.md"),
            Self::DeepSeek => include_str!("../templates/prompt_profiles/deepseek/base.md"),
            Self::Glm => include_str!("../templates/prompt_profiles/glm/base.md"),
            Self::Qwen => include_str!("../templates/prompt_profiles/qwen/base.md"),
        }
    }
}

pub(crate) fn resolve_model_instructions(
    model_info: &ModelInfo,
    provider_id: &str,
    provider: &ModelProviderInfo,
    personality: Option<Personality>,
) -> String {
    let Some(profile_id) = infer_prompt_profile_id(model_info, provider_id, provider) else {
        return model_info.get_model_instructions(personality);
    };

    let catalog_instructions = model_info.get_model_instructions(personality);
    if profile_id == PromptProfileId::CodexResponses && !catalog_instructions.trim().is_empty() {
        return catalog_instructions;
    }

    let profile_instructions = profile_id.instructions().trim();
    if profile_instructions.is_empty() {
        return catalog_instructions;
    }

    tracing::debug!(
        model = %model_info.slug,
        provider_id,
        prompt_profile = profile_id.as_str(),
        "resolved model prompt profile"
    );
    profile_instructions.to_string()
}

pub(crate) fn infer_prompt_profile_id(
    model_info: &ModelInfo,
    provider_id: &str,
    provider: &ModelProviderInfo,
) -> Option<PromptProfileId> {
    let model = model_info.slug.to_ascii_lowercase();
    let provider_id = provider_id.to_ascii_lowercase();
    let provider_name = provider.name.to_ascii_lowercase();
    let base_url = provider
        .base_url
        .as_deref()
        .unwrap_or_default()
        .to_ascii_lowercase();

    if contains_any(
        &[&model, &provider_id, &provider_name, &base_url],
        &["deepseek"],
    ) {
        return Some(PromptProfileId::DeepSeek);
    }

    if contains_any(
        &[&model, &provider_id, &provider_name, &base_url],
        &["qwen", "dashscope", "alibaba", "aliyun"],
    ) {
        return Some(PromptProfileId::Qwen);
    }

    if contains_any(
        &[&model, &provider_id, &provider_name, &base_url],
        &["glm", "bigmodel", "z.ai", "zai"],
    ) {
        return Some(PromptProfileId::Glm);
    }

    if provider.wire_api == WireApi::Responses
        && (provider_id == OPENAI_PROVIDER_ID
            || model.contains("codex")
            || model.starts_with("gpt-"))
    {
        return Some(PromptProfileId::CodexResponses);
    }

    if provider.wire_api == WireApi::Common {
        return Some(PromptProfileId::CommonBase);
    }

    None
}

fn contains_any(haystacks: &[&str], needles: &[&str]) -> bool {
    haystacks
        .iter()
        .any(|haystack| needles.iter().any(|needle| haystack.contains(needle)))
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
    fn deepseek_provider_resolves_deepseek_profile() {
        let (provider_id, provider) =
            provider("deepseek", "https://api.deepseek.com", WireApi::Common);
        let profile = infer_prompt_profile_id(&model("deepseek-v4-pro"), &provider_id, &provider);
        assert_eq!(profile, Some(PromptProfileId::DeepSeek));
    }

    #[test]
    fn unknown_common_provider_resolves_common_profile() {
        let (provider_id, provider) =
            provider("custom-common", "https://example.test/v1", WireApi::Common);
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
    fn empty_common_profile_falls_back_to_model_instructions() {
        let (provider_id, provider) =
            provider("custom-common", "https://example.test/v1", WireApi::Common);
        let model_info = model("custom-model");
        let instructions = resolve_model_instructions(&model_info, &provider_id, &provider, None);
        assert_eq!(instructions, model_info.get_model_instructions(None));
    }
}
