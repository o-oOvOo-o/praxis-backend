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

pub(in crate::praxis::session_startup) struct SessionHandleInput<'a> {
    pub(in crate::praxis::session_startup) conversation_id: ThreadId,
    pub(in crate::praxis::session_startup) tx_event: Sender<Event>,
    pub(in crate::praxis::session_startup) agent_status: watch::Sender<AgentStatus>,
    pub(in crate::praxis::session_startup) config: &'a Config,
    pub(in crate::praxis::session_startup) session_configuration: &'a SessionConfiguration,
    pub(in crate::praxis::session_startup) state: SessionState,
    pub(in crate::praxis::session_startup) services: SessionServices,
    pub(in crate::praxis::session_startup) llm_runtime_catalog: LlmRuntimeCatalog,
}
