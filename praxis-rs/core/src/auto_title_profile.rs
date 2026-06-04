use crate::model_provider_info::ModelProviderInfo;
use crate::model_provider_info::WireApi;
use crate::prompt_profiles::PromptProfileId;
use crate::prompt_profiles::infer_prompt_profile_id;
use praxis_protocol::openai_models::ModelInfo;
use praxis_protocol::openai_models::ReasoningEffort;

pub(crate) const DEEPSEEK_AUTO_TITLE_MODEL: &str = "deepseek-v4-flash";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AutoTitleProfile {
    CodexResponses,
    DeepSeekFlash,
    Common,
    ProviderDefault,
}

impl AutoTitleProfile {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::CodexResponses => "codex/responses",
            Self::DeepSeekFlash => "deepseek/flash",
            Self::Common => "common/current",
            Self::ProviderDefault => "provider/current",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AutoTitleModelSelection {
    pub(crate) model_slug: String,
    pub(crate) profile: AutoTitleProfile,
    pub(crate) reasoning_effort: Option<ReasoningEffort>,
    pub(crate) suppress_model_default_reasoning: bool,
}

pub(crate) fn select_auto_title_model(
    current_model: &ModelInfo,
    provider_id: &str,
    provider: &ModelProviderInfo,
) -> AutoTitleModelSelection {
    match infer_prompt_profile_id(current_model, provider_id, provider) {
        Some(PromptProfileId::DeepSeek) => AutoTitleModelSelection {
            model_slug: DEEPSEEK_AUTO_TITLE_MODEL.to_string(),
            profile: AutoTitleProfile::DeepSeekFlash,
            reasoning_effort: None,
            suppress_model_default_reasoning: true,
        },
        Some(PromptProfileId::CodexResponses) => AutoTitleModelSelection {
            model_slug: current_model.slug.clone(),
            profile: AutoTitleProfile::CodexResponses,
            reasoning_effort: None,
            suppress_model_default_reasoning: false,
        },
        Some(PromptProfileId::CommonBase | PromptProfileId::Glm | PromptProfileId::Qwen) => {
            AutoTitleModelSelection {
                model_slug: current_model.slug.clone(),
                profile: AutoTitleProfile::Common,
                reasoning_effort: None,
                suppress_model_default_reasoning: false,
            }
        }
        None => AutoTitleModelSelection {
            model_slug: current_model.slug.clone(),
            profile: match provider.wire_api {
                WireApi::Common => AutoTitleProfile::Common,
                _ => AutoTitleProfile::ProviderDefault,
            },
            reasoning_effort: None,
            suppress_model_default_reasoning: false,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn deepseek_title_uses_flash_without_default_reasoning() {
        let (provider_id, provider) =
            provider("deepseek", "https://api.deepseek.com", WireApi::Common);

        let selection = select_auto_title_model(&model("deepseek-v4-pro"), &provider_id, &provider);

        assert_eq!(selection.model_slug, DEEPSEEK_AUTO_TITLE_MODEL);
        assert_eq!(selection.profile, AutoTitleProfile::DeepSeekFlash);
        assert_eq!(selection.reasoning_effort, None);
        assert!(selection.suppress_model_default_reasoning);
    }

    #[test]
    fn responses_title_keeps_current_model() {
        let (provider_id, provider) =
            provider("openai", "https://api.openai.com/v1", WireApi::Responses);

        let selection = select_auto_title_model(&model("gpt-5.2-codex"), &provider_id, &provider);

        assert_eq!(selection.model_slug, "gpt-5.2-codex");
        assert_eq!(selection.profile, AutoTitleProfile::CodexResponses);
        assert!(!selection.suppress_model_default_reasoning);
    }

    #[test]
    fn generic_common_title_keeps_current_model() {
        let (provider_id, provider) =
            provider("custom-common", "https://example.test/v1", WireApi::Common);

        let selection = select_auto_title_model(&model("custom-model"), &provider_id, &provider);

        assert_eq!(selection.model_slug, "custom-model");
        assert_eq!(selection.profile, AutoTitleProfile::Common);
        assert!(!selection.suppress_model_default_reasoning);
    }
}
