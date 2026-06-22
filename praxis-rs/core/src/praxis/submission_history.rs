use std::sync::Arc;

use crate::config::Config;
use crate::praxis::Session;
use praxis_protocol::protocol::Event;
use praxis_protocol::protocol::EventMsg;
use praxis_protocol::protocol::GetHistoryEntryResponseEvent;
use tracing::warn;

pub(super) async fn add_to_history(sess: &Arc<Session>, config: &Arc<Config>, text: String) {
    let id = sess.conversation_id;
    let config = Arc::clone(config);
    tokio::spawn(async move {
        if let Err(e) = crate::message_history::append_entry(&text, &id, &config).await {
            warn!("failed to append to message history: {e}");
        }
    });
}

pub(super) async fn get_history_entry_request(
    sess: &Arc<Session>,
    config: &Arc<Config>,
    sub_id: String,
    offset: usize,
    log_id: u64,
) {
    let config = Arc::clone(config);
    let sess_clone = Arc::clone(sess);

    tokio::spawn(async move {
        let entry_opt = tokio::task::spawn_blocking(move || {
            crate::message_history::lookup(log_id, offset, &config)
        })
        .await
        .unwrap_or(None);

        let event = Event {
            id: sub_id,
            msg: EventMsg::GetHistoryEntryResponse(GetHistoryEntryResponseEvent {
                offset,
                log_id,
                entry: entry_opt.map(|e| praxis_protocol::message_history::HistoryEntry {
                    conversation_id: e.session_id,
                    ts: e.ts,
                    text: e.text,
                }),
            }),
        };

        sess_clone.send_event_raw(event).await;
    });
}
