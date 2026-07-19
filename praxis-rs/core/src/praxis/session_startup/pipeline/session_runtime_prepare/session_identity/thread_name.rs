use std::sync::Arc;
use std::time::Duration;

use praxis_protocol::ThreadId;
use praxis_protocol::protocol::InitialHistory;
use praxis_rollout::state_db::StateDbHandle;
use tracing::warn;

use crate::config::Config;
use crate::praxis::SessionConfiguration;

use super::super::super::super::thread_name_bootstrap;

pub(super) struct ThreadNameInput<'a> {
    pub(super) conversation_id: ThreadId,
    pub(super) forked_from_id: Option<ThreadId>,
    pub(super) initial_history: &'a InitialHistory,
    pub(super) state_db_ctx: &'a Option<StateDbHandle>,
    pub(super) config: &'a Arc<Config>,
    pub(super) session_configuration: &'a mut SessionConfiguration,
}

pub(super) async fn resolve_and_assign(input: ThreadNameInput<'_>) {
    if matches!(input.initial_history, InitialHistory::New) {
        input.session_configuration.thread_name = None;
        return;
    }

    const THREAD_NAME_LOOKUP_TIMEOUT: Duration = Duration::from_secs(1);
    let conversation_id = input.conversation_id;
    let forked_from_id = input.forked_from_id;
    let initial_history = input.initial_history.clone();
    let state_db_ctx = input.state_db_ctx.clone();
    let ephemeral = input.config.ephemeral;
    let lookup_runtime = tokio::runtime::Handle::current();
    let mut lookup = tokio::task::spawn_blocking(move || {
        lookup_runtime.block_on(thread_name_bootstrap::resolve_session_thread_name(
            conversation_id,
            forked_from_id,
            &initial_history,
            state_db_ctx.as_deref(),
            ephemeral,
        ))
    });
    match tokio::time::timeout(
        THREAD_NAME_LOOKUP_TIMEOUT,
        &mut lookup,
    )
    .await
    {
        Ok(Ok(thread_name)) => input.session_configuration.thread_name = thread_name,
        Ok(Err(error)) => warn!(
            conversation_id = %input.conversation_id,
            %error,
            "Thread name lookup task failed; continuing session startup without a cached name"
        ),
        Err(_) => {
            lookup.abort();
            warn!(
                conversation_id = %input.conversation_id,
                timeout_ms = THREAD_NAME_LOOKUP_TIMEOUT.as_millis(),
                "Thread name lookup timed out; continuing session startup without a cached name"
            );
        }
    }
}
