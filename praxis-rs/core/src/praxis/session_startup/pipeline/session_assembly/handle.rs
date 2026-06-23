use std::sync::Arc;

use async_channel::Sender;
use praxis_protocol::ThreadId;
use praxis_protocol::protocol::Event;
use tokio::sync::watch;

use crate::agent::AgentStatus;
use crate::config::Config;
use crate::llm::runtime::LlmRuntimeCatalog;
use crate::praxis::Session;
use crate::praxis::SessionConfiguration;
use crate::state::SessionServices;
use crate::state::SessionState;

use super::super::super::network_proxy;
use super::super::super::session_handle;

pub(super) struct SessionHandleAssemblyInput<'a> {
    pub(super) conversation_id: ThreadId,
    pub(super) tx_event: &'a Sender<Event>,
    pub(super) agent_status: watch::Sender<AgentStatus>,
    pub(super) config: &'a Arc<Config>,
    pub(super) session_configuration: &'a SessionConfiguration,
    pub(super) state: SessionState,
    pub(super) services: SessionServices,
    pub(super) llm_runtime_catalog: LlmRuntimeCatalog,
    pub(super) network_policy_decider_session: network_proxy::PolicyDeciderSession,
}

pub(super) async fn build_and_bind(
    input: SessionHandleAssemblyInput<'_>,
) -> anyhow::Result<Arc<Session>> {
    let session = session_handle::build(session_handle::SessionHandleInput {
        conversation_id: input.conversation_id,
        tx_event: input.tx_event.clone(),
        agent_status: input.agent_status,
        config: input.config.as_ref(),
        session_configuration: input.session_configuration,
        state: input.state,
        services: input.services,
        llm_runtime_catalog: input.llm_runtime_catalog,
    });
    network_proxy::bind_session(input.network_policy_decider_session, &session).await;
    Ok(session)
}
