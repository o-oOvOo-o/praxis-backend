use std::sync::Arc;

use praxis_protocol::protocol::InitialHistory;

use crate::praxis::Session;

pub(super) async fn record(session: &Arc<Session>, initial_history: InitialHistory) {
    let session_start_source = session_start_source(&initial_history);
    session.record_initial_history(initial_history).await;

    let mut state = session.state.lock().await;
    state.set_pending_session_start_source(Some(session_start_source));
}

fn session_start_source(initial_history: &InitialHistory) -> praxis_hooks::SessionStartSource {
    match initial_history {
        InitialHistory::Resumed(_) => praxis_hooks::SessionStartSource::Resume,
        InitialHistory::New | InitialHistory::Forked(_) => {
            praxis_hooks::SessionStartSource::Startup
        }
    }
}
