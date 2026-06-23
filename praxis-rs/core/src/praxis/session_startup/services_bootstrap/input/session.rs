use std::sync::Arc;

use praxis_protocol::ThreadId;

use crate::config::Config;
use crate::praxis::SessionConfiguration;

pub(in crate::praxis::session_startup) struct ServiceSessionSpec {
    pub(in crate::praxis::session_startup) config: Arc<Config>,
    pub(in crate::praxis::session_startup) conversation_id: ThreadId,
    pub(in crate::praxis::session_startup) session_configuration: SessionConfiguration,
}
