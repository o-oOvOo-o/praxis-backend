use std::sync::Arc;

use async_channel::Sender;
use praxis_protocol::ThreadId;
use praxis_protocol::protocol::Event;
use tokio::sync::watch;

use crate::agent::AgentStatus;
use crate::config::Config;
use crate::llm::runtime::LlmRuntimeCatalog;

use super::super::super::super::super::SessionConfiguration;
use super::super::super::super::network_proxy;

pub(in crate::praxis::session_startup::pipeline) struct SessionAssemblyHandle<'a> {
    pub(in crate::praxis::session_startup::pipeline) conversation_id: ThreadId,
    pub(in crate::praxis::session_startup::pipeline) tx_event: &'a Sender<Event>,
    pub(in crate::praxis::session_startup::pipeline) agent_status: watch::Sender<AgentStatus>,
    pub(in crate::praxis::session_startup::pipeline) config: &'a Arc<Config>,
    pub(in crate::praxis::session_startup::pipeline) session_configuration:
        &'a SessionConfiguration,
    pub(in crate::praxis::session_startup::pipeline) llm_runtime_catalog: LlmRuntimeCatalog,
    pub(in crate::praxis::session_startup::pipeline) network_policy_decider_session:
        network_proxy::PolicyDeciderSession,
}
