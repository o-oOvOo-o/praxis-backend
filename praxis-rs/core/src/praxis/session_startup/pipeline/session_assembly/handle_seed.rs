use std::sync::Arc;

use async_channel::Sender;
use praxis_protocol::ThreadId;
use praxis_protocol::protocol::Event;
use tokio::sync::watch;

use crate::agent::AgentStatus;
use crate::config::Config;
use crate::llm::runtime::LlmRuntimeCatalog;
use crate::praxis::SessionConfiguration;
use crate::state::SessionServices;
use crate::state::SessionState;

use super::super::super::network_proxy;
use super::handle;

pub(super) struct SessionHandleSeed<'a> {
    pub(super) conversation_id: ThreadId,
    pub(super) tx_event: &'a Sender<Event>,
    pub(super) agent_status: watch::Sender<AgentStatus>,
    pub(super) config: &'a Arc<Config>,
    pub(super) session_configuration: &'a SessionConfiguration,
    pub(super) llm_runtime_catalog: LlmRuntimeCatalog,
    pub(super) network_policy_decider_session: network_proxy::PolicyDeciderSession,
}

impl<'a> SessionHandleSeed<'a> {
    pub(super) fn into_handle_input(
        self,
        state: SessionState,
        services: SessionServices,
    ) -> handle::SessionHandleAssemblyInput<'a> {
        handle::SessionHandleAssemblyInput {
            conversation_id: self.conversation_id,
            tx_event: self.tx_event,
            agent_status: self.agent_status,
            config: self.config,
            session_configuration: self.session_configuration,
            state,
            services,
            llm_runtime_catalog: self.llm_runtime_catalog,
            network_policy_decider_session: self.network_policy_decider_session,
        }
    }
}
