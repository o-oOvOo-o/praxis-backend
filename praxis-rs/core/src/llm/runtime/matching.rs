use praxis_plugin::PluginLlmModelCatalog;
use praxis_plugin::PluginLlmProduct;
use praxis_plugin::PluginLlmProfile;

use super::normalization::normalize_profile_id;
use super::normalization::normalize_selector;
use super::normalization::selector_eq;
use crate::llm::ids::BehaviorProfileId;
use crate::llm::ids::LEGACY_CODEX_RESPONSES_BASE_PROFILE_ID;
use crate::llm::ids::LEGACY_CODEX_RESPONSES_PROFILE_ID;
use crate::llm::ids::OPENAI_RESPONSES_BASE_PROFILE_ID;
use crate::llm::ids::OPENAI_RESPONSES_PROFILE_ID;
use crate::llm::ids::ProductProfileId;
use crate::llm::profiles::plugin::ProfileDescriptor;
use crate::model_provider_info::ModelProviderInfo;

pub(super) fn plugin_profile_matches(
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

pub(super) fn plugin_model_catalog_matches(
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

pub(super) fn profile_id_matches(plugin_profile_id: &str, behavior_id: BehaviorProfileId) -> bool {
    let plugin_profile_id = normalize_profile_id(plugin_profile_id);
    behavior_profile_aliases(behavior_id)
        .iter()
        .any(|alias| plugin_profile_id == *alias)
}

fn behavior_profile_aliases(behavior_id: BehaviorProfileId) -> &'static [&'static str] {
    match behavior_id {
        BehaviorProfileId::OpenAiResponses => &[
            OPENAI_RESPONSES_PROFILE_ID,
            OPENAI_RESPONSES_BASE_PROFILE_ID,
            LEGACY_CODEX_RESPONSES_PROFILE_ID,
            LEGACY_CODEX_RESPONSES_BASE_PROFILE_ID,
        ],
        BehaviorProfileId::Common => &["common", "common/base"],
        BehaviorProfileId::DeepSeek => &["deepseek", "deepseek/base"],
        BehaviorProfileId::Gemini => &["gemini", "gemini/base"],
        BehaviorProfileId::Glm => &["glm", "glm/base"],
        BehaviorProfileId::Qwen => &["qwen", "qwen/base"],
        BehaviorProfileId::Claude => &["claude", "claude/base"],
        BehaviorProfileId::OpenRouter => &["openrouter", "openrouter/base"],
    }
}

pub(super) fn product_id_matches(
    plugin_product: &PluginLlmProduct,
    product: ProductProfileId,
) -> bool {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn openai_responses_profile_keeps_legacy_codex_aliases() {
        assert!(profile_id_matches(
            "openai/responses",
            BehaviorProfileId::OpenAiResponses
        ));
        assert!(profile_id_matches(
            OPENAI_RESPONSES_BASE_PROFILE_ID,
            BehaviorProfileId::OpenAiResponses
        ));
        assert!(profile_id_matches(
            "codex/responses",
            BehaviorProfileId::OpenAiResponses
        ));
        assert!(profile_id_matches(
            "codex/responses/base",
            BehaviorProfileId::OpenAiResponses
        ));
    }
}
