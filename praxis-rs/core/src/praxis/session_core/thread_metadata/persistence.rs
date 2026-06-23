use std::io;

use crate::praxis::Session;

pub(super) async fn persist_thread_name(session: &Session, name: &str) -> io::Result<()> {
    praxis_rollout::ThreadNameWriter::new(session.services.state_db.as_deref())
        .write_name(session.conversation_id, name)
        .await
}

pub(super) async fn set_thread_name_in_state(session: &Session, name: String) {
    let mut state = session.state.lock().await;
    state.session_configuration.thread_name = Some(name);
}

pub(super) async fn rollout_persistence_enabled(session: &Session) -> bool {
    let rollout = session.services.rollout.lock().await;
    rollout.is_some()
}
