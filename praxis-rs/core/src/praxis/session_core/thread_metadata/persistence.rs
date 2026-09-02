use std::io;

use crate::praxis::Session;

pub(super) async fn persist_thread_name(session: &Session, name: &str) -> io::Result<()> {
    let rollout = session.services.rollout.lock().await;
    if let Some(recorder) = rollout.as_ref() {
        return recorder.set_thread_name(name.to_owned()).await;
    }
    drop(rollout);
    let praxis_home = session.praxis_home().await;
    praxis_rollout::ThreadNameWriter::with_praxis_home(
        session.services.state_db.as_deref(),
        praxis_home.as_path(),
    )
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
