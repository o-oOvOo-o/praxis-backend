use std::sync::Arc;

use praxis_protocol::protocol::Event;
use praxis_protocol::protocol::EventMsg;

use crate::praxis::Session;
use crate::skills_watcher::SkillsWatcherEvent;

pub(super) fn start_listener(session: &Arc<Session>) {
    let mut rx = session.services.skills_watcher.subscribe();
    let weak_sess = Arc::downgrade(session);
    tokio::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(SkillsWatcherEvent::SkillsChanged { .. }) => {
                    let Some(sess) = weak_sess.upgrade() else {
                        break;
                    };
                    let event = Event {
                        id: sess.next_internal_sub_id(),
                        msg: EventMsg::SkillsUpdateAvailable,
                    };
                    sess.send_event_raw(event).await;
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
            }
        }
    });
}
