use anyhow::Context;
use praxis_capability_runtime::CapabilityId;
use praxis_capability_runtime::CapabilityKind;
use praxis_capability_runtime::CapabilityManifest;
use praxis_capability_runtime::CapabilityOwnerId;
use praxis_capability_runtime::CapabilityRuntime;
use praxis_capability_runtime::ScopeId;
use praxis_capability_runtime::TypedCapability;
use std::sync::Arc;

use crate::models_manager::manager::ModelsManager;

pub type ProviderCapability = TypedCapability<Arc<ModelsManager>>;

pub(crate) fn publish_providers(
    runtime: &CapabilityRuntime,
    models_manager: Arc<ModelsManager>,
) -> anyhow::Result<ProviderCapability> {
    let capability_id = CapabilityId::new("praxis.core.providers")?;
    let owner_id = CapabilityOwnerId::new("praxis.core.providers")?;
    let manifest = CapabilityManifest::new(
        capability_id.clone(),
        CapabilityKind::Provider,
        owner_id.clone(),
        ScopeId::process(),
    );
    let mut transaction = runtime.begin_transaction(owner_id, ScopeId::process());
    transaction.stage_typed(manifest, move || Ok((models_manager, Box::new(|| Ok(())))))?;
    transaction.commit()?;
    runtime
        .acquire_typed(&ScopeId::process(), &capability_id)?
        .context("Providers capability was not active after a successful commit")
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use praxis_capability_runtime::CapabilityLifecycle;
    use praxis_login::AuthManager;
    use praxis_login::OpenAiAccountAuth;

    use crate::models_manager::collaboration_mode_presets::CollaborationModesConfig;

    #[test]
    fn providers_are_exposed_only_through_a_typed_active_generation() {
        let auth =
            AuthManager::from_auth_for_testing(OpenAiAccountAuth::from_api_key("test-api-key"));
        let manager = Arc::new(ModelsManager::new(
            std::env::temp_dir(),
            auth,
            None,
            CollaborationModesConfig::default(),
        ));
        let providers = crate::capabilities::test_provider_capability(manager);

        assert_eq!(providers.lease().lifecycle(), CapabilityLifecycle::Active);
        assert_eq!(
            providers.lease().capability_id().as_str(),
            "praxis.core.providers"
        );
    }
}
