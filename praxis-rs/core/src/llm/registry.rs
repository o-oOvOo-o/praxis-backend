use std::collections::HashMap;
use std::sync::OnceLock;

use crate::model_provider_info::ModelProviderInfo;

use super::internal_plugins;
use super::internal_plugins::LlmModelCatalogDescriptor;
use super::internal_plugins::LlmModelCatalogScope;
use super::profiles::plugin::ProfileDescriptor;
use super::profiles::plugin::ProfileMatchContext;
use super::profiles::plugin::ProfileProviderPolicy;

#[derive(Debug, Clone)]
pub(crate) struct ModelProviderSwitch {
    pub(crate) provider_id: String,
    pub(crate) provider: ModelProviderInfo,
    pub(crate) model_owner_label: &'static str,
}

#[derive(Debug)]
pub(crate) struct LlmProfileRegistry {
    profiles: Vec<ProfileDescriptor>,
    model_catalogs: Vec<LlmModelCatalogDescriptor>,
}

impl LlmProfileRegistry {
    pub(crate) fn builtin() -> Self {
        let plugin_registry = internal_plugins::builtin_registry();
        let mut profiles = plugin_registry.profiles().to_vec();
        profiles.sort_by(|left, right| right.priority.cmp(&left.priority));
        Self {
            profiles,
            model_catalogs: plugin_registry.model_catalogs().to_vec(),
        }
    }

    pub(crate) fn builtin_static() -> &'static Self {
        static REGISTRY: OnceLock<LlmProfileRegistry> = OnceLock::new();
        REGISTRY.get_or_init(Self::builtin)
    }

    pub(crate) fn resolve(&self, ctx: &ProfileMatchContext<'_>) -> Option<ProfileDescriptor> {
        self.profiles
            .iter()
            .copied()
            .find(|profile| profile.matches(ctx))
    }

    pub(crate) fn model_catalogs(&self) -> &[LlmModelCatalogDescriptor] {
        &self.model_catalogs
    }

    pub(crate) fn provider_accepts_known_first_party_model(
        &self,
        provider_id: &str,
        provider: &ModelProviderInfo,
        model: &str,
    ) -> bool {
        provider_accepts_model_from_catalogs(&self.model_catalogs, provider_id, provider, model)
    }

    pub(crate) fn provider_switch_for_selected_model(
        &self,
        current_provider_id: &str,
        current_provider: &ModelProviderInfo,
        model: &str,
        model_providers: &HashMap<String, ModelProviderInfo>,
    ) -> Option<ModelProviderSwitch> {
        let model_policy = self.first_party_policy_for_model(model)?;
        if self
            .first_party_policy_for_provider(current_provider_id, current_provider)
            .is_some_and(|current_policy| current_policy.owner_label == model_policy.owner_label)
        {
            return None;
        }

        self.find_provider_for_policy(model_policy, model_providers)
            .filter(|candidate| candidate.0 != current_provider_id)
            .map(|(provider_id, provider)| ModelProviderSwitch {
                provider_id,
                provider,
                model_owner_label: model_policy.owner_label,
            })
    }

    pub(crate) fn first_party_policy_for_provider(
        &self,
        provider_id: &str,
        provider: &ModelProviderInfo,
    ) -> Option<ProfileProviderPolicy> {
        self.profiles.iter().find_map(|profile| {
            let policy = profile.provider_policy?;
            policy
                .matches_provider(provider_id, provider)
                .then_some(policy)
        })
    }

    pub(crate) fn first_party_policy_for_model(
        &self,
        model: &str,
    ) -> Option<ProfileProviderPolicy> {
        self.profiles.iter().find_map(|profile| {
            let policy = profile.provider_policy?;
            policy.matches_model(model).then_some(policy)
        })
    }

    fn find_provider_for_policy(
        &self,
        policy: ProfileProviderPolicy,
        model_providers: &HashMap<String, ModelProviderInfo>,
    ) -> Option<(String, ModelProviderInfo)> {
        if let Some(provider_id) = policy.canonical_provider_id
            && let Some(provider) = model_providers.get(provider_id)
            && policy.matches_provider(provider_id, provider)
        {
            return Some((provider_id.to_string(), provider.clone()));
        }

        model_providers
            .iter()
            .find(|(provider_id, provider)| policy.matches_provider(provider_id, provider))
            .map(|(provider_id, provider)| (provider_id.clone(), provider.clone()))
    }
}

pub(crate) fn provider_accepts_model_from_catalogs(
    catalogs: &[LlmModelCatalogDescriptor],
    provider_id: &str,
    provider: &ModelProviderInfo,
    model: &str,
) -> bool {
    let model = model.trim();
    if model.is_empty() {
        return false;
    }

    let provider_exclusive_catalogs = catalogs
        .iter()
        .copied()
        .filter(|catalog| {
            matches!(
                catalog.scope,
                LlmModelCatalogScope::Exclusive | LlmModelCatalogScope::ProviderExclusive
            ) && catalog.matches_provider(provider_id, provider)
        })
        .collect::<Vec<_>>();
    if !provider_exclusive_catalogs.is_empty() {
        return provider_exclusive_catalogs
            .iter()
            .copied()
            .any(|catalog| catalog.matches_model(model));
    }

    let model_exclusive_catalogs = catalogs
        .iter()
        .copied()
        .filter(|catalog| {
            catalog.scope == LlmModelCatalogScope::Exclusive && catalog.matches_model(model)
        })
        .collect::<Vec<_>>();
    if !model_exclusive_catalogs.is_empty() {
        return model_exclusive_catalogs
            .iter()
            .copied()
            .any(|catalog| catalog.matches_provider(provider_id, provider));
    }

    let provider_catalogs = catalogs
        .iter()
        .copied()
        .filter(|catalog| catalog.matches_provider(provider_id, provider))
        .collect::<Vec<_>>();
    if provider_catalogs.is_empty() {
        return true;
    }

    provider_catalogs
        .iter()
        .copied()
        .any(|catalog| catalog.matches_model(model))
}
