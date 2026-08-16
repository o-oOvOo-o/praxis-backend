use std::ops::Deref;
use std::sync::Arc;

use anyhow::Context;
use praxis_capability_runtime::CapabilityId;
use praxis_capability_runtime::CapabilityKind;
use praxis_capability_runtime::CapabilityManifest;
use praxis_capability_runtime::CapabilityOwnerId;
use praxis_capability_runtime::CapabilityScope;
use praxis_capability_runtime::ScopeId;
use praxis_capability_runtime::ScopeKind;
use praxis_capability_runtime::TypedCapability;
use praxis_protocol::ThreadId;

use crate::tools::ToolRouter;

const MODEL_TOOLS_ID: &str = "praxis.core.tools";
const CODE_MODE_TOOLS_ID: &str = "praxis.core.tools.code_mode";
const TOOLS_OWNER_ID: &str = "praxis.core.tools";

#[derive(Clone)]
pub(crate) struct ToolCapability {
    router: TypedCapability<ToolRouter>,
    _scope: Arc<CapabilityScope>,
}

impl ToolCapability {
    pub(crate) fn router(&self) -> &ToolRouter {
        self.router.value()
    }
}

impl Deref for ToolCapability {
    type Target = ToolRouter;

    fn deref(&self) -> &Self::Target {
        self.router()
    }
}

pub(crate) struct ToolCapabilities {
    model: ToolCapability,
    code_mode: ToolCapability,
}

impl ToolCapabilities {
    pub(crate) fn model(&self) -> ToolCapability {
        self.model.clone()
    }

    pub(crate) fn code_mode(&self) -> ToolCapability {
        self.code_mode.clone()
    }
}

impl AsRef<ToolRouter> for ToolCapabilities {
    fn as_ref(&self) -> &ToolRouter {
        self.model.router()
    }
}

impl Deref for ToolCapabilities {
    type Target = ToolRouter;

    fn deref(&self) -> &Self::Target {
        self.as_ref()
    }
}

pub(crate) fn publish_tools(
    thread_scope: &CapabilityScope,
    conversation_id: ThreadId,
    turn_id: &str,
    model: ToolRouter,
    code_mode: ToolRouter,
) -> anyhow::Result<ToolCapabilities> {
    let scope = Arc::new(open_turn_scope(thread_scope, conversation_id, turn_id)?);
    let model_id = CapabilityId::new(MODEL_TOOLS_ID)?;
    let code_mode_id = CapabilityId::new(CODE_MODE_TOOLS_ID)?;
    let owner_id = CapabilityOwnerId::new(TOOLS_OWNER_ID)?;
    let hooks_id = CapabilityId::new("praxis.core.hooks")?;
    let mut transaction = scope.begin_transaction(owner_id.clone());
    transaction.stage_typed(
        CapabilityManifest::new(
            model_id.clone(),
            CapabilityKind::Tool,
            owner_id.clone(),
            scope.id().clone(),
        )
        .with_dependencies([hooks_id, CapabilityId::new("praxis.core.skills.resolved")?]),
        move || Ok((model, Box::new(|| Ok(())))),
    )?;
    transaction.stage_typed(
        CapabilityManifest::new(
            code_mode_id.clone(),
            CapabilityKind::Tool,
            owner_id,
            scope.id().clone(),
        )
        .with_dependencies([model_id.clone()]),
        move || Ok((code_mode, Box::new(|| Ok(())))),
    )?;
    transaction.commit()?;

    Ok(ToolCapabilities {
        model: acquire(&scope, &model_id)?,
        code_mode: acquire(&scope, &code_mode_id)?,
    })
}

fn open_turn_scope(
    thread_scope: &CapabilityScope,
    conversation_id: ThreadId,
    turn_id: &str,
) -> anyhow::Result<CapabilityScope> {
    Ok(thread_scope.runtime().open_child_scope(
        ScopeId::new(format!("turn:{conversation_id}:{turn_id}"))?,
        ScopeKind::Turn,
        thread_scope.id().clone(),
    )?)
}

fn acquire(
    scope: &Arc<CapabilityScope>,
    capability_id: &CapabilityId,
) -> anyhow::Result<ToolCapability> {
    let router = scope
        .acquire_typed(capability_id)?
        .with_context(|| format!("{capability_id} was not active after a successful commit"))?;
    Ok(ToolCapability {
        router,
        _scope: Arc::clone(scope),
    })
}

#[cfg(test)]
mod tests {
    use praxis_capability_runtime::CapabilityLifecycle;

    use crate::tools::ToolRouter;
    use crate::tools::router::ToolRouterParams;

    #[tokio::test]
    async fn tools_are_published_as_one_typed_turn_generation() {
        let (session, turn) = crate::praxis::make_session_and_context().await;
        let build_router = || {
            ToolRouter::from_config(
                &turn.tools_config,
                ToolRouterParams {
                    mcp_tools: None,
                    app_tools: None,
                    discoverable_tools: None,
                    dynamic_tools: turn.dynamic_tools.as_slice(),
                    tool_visibility_policy: None,
                },
            )
        };
        let tools = super::publish_tools(
            &session.services._capability_scope,
            session.conversation_id,
            turn.sub_id.as_str(),
            build_router(),
            build_router(),
        )
        .expect("publish test Tools capabilities");

        assert_eq!(
            tools.model.router.lease().lifecycle(),
            CapabilityLifecycle::Active
        );
        assert_eq!(
            tools.model.router.lease().generation_id(),
            tools.code_mode.router.lease().generation_id()
        );
        assert_eq!(
            tools.model.router.lease().capability_id().as_str(),
            super::MODEL_TOOLS_ID
        );
    }

    #[tokio::test]
    async fn replaced_tools_quiesce_until_the_last_consumer_drops() {
        let (session, turn) = crate::praxis::make_session_and_context().await;
        let build_router = || {
            ToolRouter::from_config(
                &turn.tools_config,
                ToolRouterParams {
                    mcp_tools: None,
                    app_tools: None,
                    discoverable_tools: None,
                    dynamic_tools: turn.dynamic_tools.as_slice(),
                    tool_visibility_policy: None,
                },
            )
        };
        let first = super::publish_tools(
            &session.services._capability_scope,
            session.conversation_id,
            turn.sub_id.as_str(),
            build_router(),
            build_router(),
        )
        .expect("publish first Tools generation");
        let old_consumer = first.model();
        let old_generation = old_consumer.router.lease().generation_id();
        let runtime = session.services._capability_scope.runtime();

        let second = super::publish_tools(
            &session.services._capability_scope,
            session.conversation_id,
            turn.sub_id.as_str(),
            build_router(),
            build_router(),
        )
        .expect("publish replacement Tools generation");

        assert_eq!(
            old_consumer.router.lease().lifecycle(),
            CapabilityLifecycle::Quiescing
        );
        assert_ne!(old_generation, second.model.router.lease().generation_id());
        drop(first);
        drop(old_consumer);
        assert_eq!(
            runtime
                .snapshot()
                .generations
                .iter()
                .find(|generation| generation.id == old_generation)
                .expect("retired Tools generation remains observable")
                .lifecycle,
            CapabilityLifecycle::Disposed
        );
    }
}
