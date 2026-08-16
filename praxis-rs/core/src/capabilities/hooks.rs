use anyhow::Context;
use praxis_capability_runtime::CapabilityId;
use praxis_capability_runtime::CapabilityKind;
use praxis_capability_runtime::CapabilityManifest;
use praxis_capability_runtime::CapabilityOwnerId;
use praxis_capability_runtime::CapabilityScope;
use praxis_capability_runtime::TypedCapability;
use praxis_hooks::Hooks;

pub(crate) type HookCapability = TypedCapability<Hooks>;

pub(crate) fn publish_hooks(
    scope: &CapabilityScope,
    hooks: Hooks,
) -> anyhow::Result<HookCapability> {
    let capability_id = CapabilityId::new("praxis.core.hooks")?;
    let owner_id = CapabilityOwnerId::new("praxis.core.hooks")?;
    let manifest = CapabilityManifest::new(
        capability_id.clone(),
        CapabilityKind::Hook,
        owner_id.clone(),
        scope.id().clone(),
    );
    let mut transaction = scope.begin_transaction(owner_id);
    transaction.stage_typed(manifest, move || Ok((hooks, Box::new(|| Ok(())))))?;
    transaction.commit()?;
    scope
        .acquire_typed(&capability_id)?
        .context("Hooks capability was not active after a successful commit")
}

#[cfg(test)]
mod tests {
    use praxis_capability_runtime::CapabilityLifecycle;
    use praxis_hooks::HooksConfig;
    use praxis_protocol::ThreadId;

    #[test]
    fn hooks_are_exposed_only_through_a_typed_active_generation() {
        let (scope, hooks) = crate::capabilities::test_hook_capability(
            ThreadId::new(),
            praxis_hooks::Hooks::new(HooksConfig::default()),
        );

        assert_eq!(hooks.lease().lifecycle(), CapabilityLifecycle::Active);
        assert_eq!(hooks.lease().capability_id().as_str(), "praxis.core.hooks");
        assert_eq!(scope.id().as_str().split(':').next(), Some("thread"));
    }
}
