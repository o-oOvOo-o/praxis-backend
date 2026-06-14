use crate::outgoing_message::ThreadScopedOutgoingMessageSender;
use praxis_app_gateway_protocol::ItemCompletedNotification;
use praxis_app_gateway_protocol::ItemStartedNotification;
use praxis_app_gateway_protocol::ServerNotification;
use praxis_app_gateway_protocol::ThreadItem;
use praxis_protocol::ThreadId;

pub(crate) struct ThreadItemNotificationSink {
    outgoing: ThreadScopedOutgoingMessageSender,
    thread_id: String,
    turn_id: String,
}

impl ThreadItemNotificationSink {
    pub(crate) fn new(
        outgoing: &ThreadScopedOutgoingMessageSender,
        thread_id: &ThreadId,
        turn_id: &str,
    ) -> Self {
        Self {
            outgoing: outgoing.clone(),
            thread_id: thread_id.to_string(),
            turn_id: turn_id.to_string(),
        }
    }

    pub(crate) fn for_turn_id(&self, turn_id: impl Into<String>) -> Self {
        Self {
            outgoing: self.outgoing.clone(),
            thread_id: self.thread_id.clone(),
            turn_id: turn_id.into(),
        }
    }

    pub(crate) async fn item_started(&self, item: ThreadItem) {
        self.outgoing
            .send_server_notification(ServerNotification::ItemStarted(ItemStartedNotification {
                thread_id: self.thread_id.clone(),
                turn_id: self.turn_id.clone(),
                item,
            }))
            .await;
    }

    pub(crate) async fn item_completed(&self, item: ThreadItem) {
        self.outgoing
            .send_server_notification(ServerNotification::ItemCompleted(
                ItemCompletedNotification {
                    thread_id: self.thread_id.clone(),
                    turn_id: self.turn_id.clone(),
                    item,
                },
            ))
            .await;
    }

    pub(crate) async fn item_started_and_completed(&self, item: ThreadItem) {
        self.item_started(item.clone()).await;
        self.item_completed(item).await;
    }
}
