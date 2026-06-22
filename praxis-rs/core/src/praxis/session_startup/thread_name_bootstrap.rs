use praxis_protocol::ThreadId;
use praxis_protocol::protocol::InitialHistory;
use praxis_rollout::ThreadNameResolver;
use praxis_rollout::ThreadNameWriter;
use praxis_state::StateRuntime;
use tracing::Instrument;
use tracing::info_span;
use tracing::warn;

pub(super) async fn resolve_session_thread_name(
    conversation_id: ThreadId,
    forked_from_id: Option<ThreadId>,
    initial_history: &InitialHistory,
    state_db: Option<&StateRuntime>,
    ephemeral: bool,
) -> Option<String> {
    let resolver = ThreadNameResolver::new(state_db);
    let writer = ThreadNameWriter::new(state_db);
    let mut inherited_from_fork = false;
    let mut thread_name = resolver
        .resolve_name(conversation_id)
        .instrument(info_span!(
            "session_init.thread_name_lookup",
            otel.name = "session_init.thread_name_lookup",
        ))
        .await;

    if thread_name.is_none()
        && matches!(initial_history, InitialHistory::Forked(_))
        && let Some(source_thread_id) = forked_from_id
    {
        thread_name = resolver.resolve_name(source_thread_id).await;
        inherited_from_fork = thread_name.is_some();
    }

    if inherited_from_fork
        && !ephemeral
        && let Some(name) = thread_name.as_deref()
        && let Err(err) = writer.write_name(conversation_id, name).await
    {
        warn!("Failed to persist inherited thread name for fork {conversation_id}: {err}");
    }

    thread_name
}
