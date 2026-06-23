use praxis_protocol::protocol::SessionSource;

use crate::praxis::Session;

pub(super) async fn is_subagent_session(session: &Session) -> bool {
    let state = session.state.lock().await;
    matches!(
        state.session_configuration.session_source,
        SessionSource::SubAgent(_)
    )
}
