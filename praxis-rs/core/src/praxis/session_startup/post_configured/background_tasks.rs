use std::sync::Arc;

use crate::config::Config;
use crate::memories;
use crate::praxis::Session;
use crate::praxis::SessionConfiguration;

pub(super) async fn schedule_startup_prewarm(
    session: &Arc<Session>,
    session_configuration: &SessionConfiguration,
) {
    session
        .schedule_startup_prewarm(session_configuration.base_instructions.clone())
        .await;
}

pub(super) fn start_memory_bootstrap(
    session: &Arc<Session>,
    config: Arc<Config>,
    session_configuration: &SessionConfiguration,
) {
    memories::start_memories_startup_task(session, config, &session_configuration.session_source);
}
