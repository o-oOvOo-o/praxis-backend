use std::sync::Arc;

use crate::config::Config;
use crate::praxis::Session;
use praxis_protocol::protocol::PraxisErrorInfo;

pub(super) async fn drop_memories(sess: &Arc<Session>, config: &Arc<Config>, sub_id: String) {
    let mut errors = Vec::new();

    if let Some(state_db) = sess.services.state_db.as_deref() {
        if let Err(err) = state_db.clear_memory_data().await {
            errors.push(format!("failed clearing memory rows from state db: {err}"));
        }
    } else {
        errors.push("state db unavailable; memory rows were not cleared".to_string());
    }

    let memory_root = crate::memories::memory_root(&config.praxis_home);
    if let Err(err) = crate::memories::clear_memory_root_contents(&memory_root).await {
        errors.push(format!(
            "failed clearing memory directory {}: {err}",
            memory_root.display()
        ));
    }

    if errors.is_empty() {
        sess.raw_event_emitter(sub_id)
            .warning(format!(
                "Dropped memories at {} and cleared memory rows from state db.",
                memory_root.display()
            ))
            .await;
        return;
    }

    sess.raw_event_emitter(sub_id)
        .error(
            format!("Memory drop completed with errors: {}", errors.join("; ")),
            Some(PraxisErrorInfo::Other),
        )
        .await;
}

pub(super) async fn update_memories(sess: &Arc<Session>, config: &Arc<Config>, sub_id: String) {
    let session_source = {
        let state = sess.state.lock().await;
        state.session_configuration.session_source.clone()
    };

    crate::memories::start_memories_startup_task(sess, Arc::clone(config), &session_source);

    sess.raw_event_emitter(sub_id)
        .warning("Memory update triggered.")
        .await;
}
