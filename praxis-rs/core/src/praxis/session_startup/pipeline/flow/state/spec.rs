use praxis_protocol::protocol::InitialHistory;
use praxis_protocol::protocol::SessionSource;

use crate::llm::runtime::LlmRuntimeCatalog;
use crate::praxis::SessionConfiguration;

pub(in crate::praxis::session_startup::pipeline::flow) struct SessionStartupSpec {
    pub(in crate::praxis::session_startup::pipeline::flow) session_configuration:
        SessionConfiguration,
    pub(in crate::praxis::session_startup::pipeline::flow) llm_runtime_catalog: LlmRuntimeCatalog,
    pub(in crate::praxis::session_startup::pipeline::flow) initial_history: InitialHistory,
    pub(in crate::praxis::session_startup::pipeline::flow) session_source: SessionSource,
}
