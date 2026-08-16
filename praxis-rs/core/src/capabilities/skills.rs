use std::ops::Deref;
use std::sync::Arc;

use anyhow::Context;
use praxis_capability_runtime::CapabilityId;
use praxis_capability_runtime::CapabilityKind;
use praxis_capability_runtime::CapabilityManifest;
use praxis_capability_runtime::CapabilityOwnerId;
use praxis_capability_runtime::CapabilityRuntime;
use praxis_capability_runtime::CapabilityScope;
use praxis_capability_runtime::ScopeId;
use praxis_capability_runtime::ScopeKind;
use praxis_capability_runtime::TypedCapability;
use praxis_protocol::ThreadId;

use crate::SkillLoadOutcome;
use crate::SkillsManager;

const SKILLS_ID: &str = "praxis.core.skills";
const RESOLVED_SKILLS_ID: &str = "praxis.core.skills.resolved";
const SKILLS_OWNER_ID: &str = "praxis.core.skills";

pub type SkillsCapability = TypedCapability<Arc<SkillsManager>>;

#[derive(Clone, Debug)]
pub(crate) struct ResolvedSkillsCapability {
    outcome: TypedCapability<SkillLoadOutcome>,
    _scope: Arc<CapabilityScope>,
}

impl ResolvedSkillsCapability {
    pub(crate) fn outcome(&self) -> &SkillLoadOutcome {
        self.outcome.value()
    }
}

impl Deref for ResolvedSkillsCapability {
    type Target = SkillLoadOutcome;

    fn deref(&self) -> &Self::Target {
        self.outcome()
    }
}

impl AsRef<SkillLoadOutcome> for ResolvedSkillsCapability {
    fn as_ref(&self) -> &SkillLoadOutcome {
        self.outcome()
    }
}

pub(crate) fn publish_skills(
    runtime: &CapabilityRuntime,
    manager: Arc<SkillsManager>,
) -> anyhow::Result<SkillsCapability> {
    let capability_id = CapabilityId::new(SKILLS_ID)?;
    let owner_id = CapabilityOwnerId::new(SKILLS_OWNER_ID)?;
    let mut transaction = runtime.begin_transaction(owner_id.clone(), ScopeId::process());
    transaction.stage_typed(
        CapabilityManifest::new(
            capability_id.clone(),
            CapabilityKind::Skill,
            owner_id,
            ScopeId::process(),
        ),
        move || Ok((manager, Box::new(|| Ok(())))),
    )?;
    transaction.commit()?;
    runtime
        .acquire_typed(&ScopeId::process(), &capability_id)?
        .context("Skills capability was not active after a successful commit")
}

pub(crate) fn publish_resolved_skills(
    thread_scope: &CapabilityScope,
    conversation_id: ThreadId,
    turn_id: &str,
    outcome: SkillLoadOutcome,
) -> anyhow::Result<ResolvedSkillsCapability> {
    let scope = Arc::new(thread_scope.runtime().open_child_scope(
        ScopeId::new(format!("turn:{conversation_id}:{turn_id}"))?,
        ScopeKind::Turn,
        thread_scope.id().clone(),
    )?);
    let capability_id = CapabilityId::new(RESOLVED_SKILLS_ID)?;
    let owner_id = CapabilityOwnerId::new(SKILLS_OWNER_ID)?;
    let mut transaction = scope.begin_transaction(owner_id.clone());
    transaction.stage_typed(
        CapabilityManifest::new(
            capability_id.clone(),
            CapabilityKind::Skill,
            owner_id,
            scope.id().clone(),
        )
        .with_dependencies([CapabilityId::new(SKILLS_ID)?]),
        move || Ok((outcome, Box::new(|| Ok(())))),
    )?;
    transaction.commit()?;
    let outcome = scope
        .acquire_typed(&capability_id)?
        .context("Resolved Skills capability was not active after a successful commit")?;
    Ok(ResolvedSkillsCapability {
        outcome,
        _scope: scope,
    })
}

#[cfg(test)]
mod tests {
    use praxis_capability_runtime::CapabilityLifecycle;

    #[tokio::test]
    async fn turn_skills_hold_a_typed_resolved_generation() {
        let (_session, turn) = crate::praxis::make_session_and_context().await;

        assert_eq!(
            turn.turn_skills.outcome.outcome.lease().lifecycle(),
            CapabilityLifecycle::Active
        );
        assert_eq!(
            turn.turn_skills
                .outcome
                .outcome
                .lease()
                .capability_id()
                .as_str(),
            super::RESOLVED_SKILLS_ID
        );
    }
}
