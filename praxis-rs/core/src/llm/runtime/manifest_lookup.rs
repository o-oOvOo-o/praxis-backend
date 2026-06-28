use praxis_plugin::PluginLlmManifest;
use praxis_plugin::PluginLlmToolPolicy;
use praxis_utils_absolute_path::AbsolutePathBuf;

use super::matching::plugin_profile_matches;
use super::matching::product_id_matches;
use super::matching::profile_id_matches;
use super::prompts::read_plugin_prompt;
use super::prompts::resolve_prompt_slot;
use crate::llm::ids::BehaviorProfileId;
use crate::llm::ids::ProductProfileId;
use crate::llm::profiles::plugin::ProfileDescriptor;
use crate::llm::prompts::LlmPromptPurpose;
use crate::model_provider_info::ModelProviderInfo;

pub(super) fn resolve_profile_prompt(
    plugin_manifests: &[PluginLlmManifest],
    profile: ProfileDescriptor,
    provider_id: &str,
    provider: &ModelProviderInfo,
    purpose: LlmPromptPurpose,
) -> Option<String> {
    plugin_manifests
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

pub(super) fn resolve_product_prompt(
    plugin_manifests: &[PluginLlmManifest],
    product: &ProductProfileId,
    purpose: LlmPromptPurpose,
) -> Option<String> {
    plugin_manifests
        .iter()
        .flat_map(|manifest| manifest.products.iter())
        .filter(|plugin_product| product_id_matches(plugin_product, product))
        .find_map(|plugin_product| {
            resolve_prompt_slot(plugin_product.prompts.iter(), purpose).and_then(|slot| {
                read_plugin_prompt(
                    slot,
                    plugin_product.id.as_str(),
                    product.policy_reader_behavior_id(),
                    purpose,
                )
            })
        })
}

pub(super) fn profile_task_policy_path(
    plugin_manifests: &[PluginLlmManifest],
    profile: ProfileDescriptor,
    provider_id: &str,
    provider: &ModelProviderInfo,
) -> Option<AbsolutePathBuf> {
    find_profile(plugin_manifests, profile, provider_id, provider)
        .and_then(|plugin_profile| plugin_profile.tasks.clone())
}

pub(super) fn profile_tools_policy_path(
    plugin_manifests: &[PluginLlmManifest],
    profile: ProfileDescriptor,
    provider_id: &str,
    provider: &ModelProviderInfo,
) -> Option<AbsolutePathBuf> {
    find_profile(plugin_manifests, profile, provider_id, provider)
        .and_then(|plugin_profile| plugin_profile.tools.clone())
}

pub(super) fn product_task_policy_path(
    plugin_manifests: &[PluginLlmManifest],
    product: &ProductProfileId,
) -> Option<AbsolutePathBuf> {
    find_product(plugin_manifests, product).and_then(|plugin_product| plugin_product.tasks.clone())
}

pub(super) fn product_tools_policy_path(
    plugin_manifests: &[PluginLlmManifest],
    product: &ProductProfileId,
) -> Option<AbsolutePathBuf> {
    find_product(plugin_manifests, product).and_then(|plugin_product| plugin_product.tools.clone())
}

pub(super) fn tool_policies_for_profile(
    plugin_manifests: &[PluginLlmManifest],
    profile: BehaviorProfileId,
) -> Vec<PluginLlmToolPolicy> {
    plugin_manifests
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

fn find_profile<'a>(
    plugin_manifests: &'a [PluginLlmManifest],
    profile: ProfileDescriptor,
    provider_id: &str,
    provider: &ModelProviderInfo,
) -> Option<&'a praxis_plugin::PluginLlmProfile> {
    plugin_manifests
        .iter()
        .flat_map(|manifest| manifest.profiles.iter())
        .find(|plugin_profile| {
            plugin_profile_matches(plugin_profile, profile, provider_id, provider)
        })
}

fn find_product<'a>(
    plugin_manifests: &'a [PluginLlmManifest],
    product: &ProductProfileId,
) -> Option<&'a praxis_plugin::PluginLlmProduct> {
    plugin_manifests
        .iter()
        .flat_map(|manifest| manifest.products.iter())
        .find(|plugin_product| product_id_matches(plugin_product, product))
}
