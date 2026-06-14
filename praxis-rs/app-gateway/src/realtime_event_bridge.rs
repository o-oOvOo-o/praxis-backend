use crate::outgoing_message::ThreadScopedOutgoingMessageSender;
use praxis_app_gateway_protocol::ServerNotification;
use praxis_app_gateway_protocol::ThreadRealtimeClosedNotification;
use praxis_app_gateway_protocol::ThreadRealtimeErrorNotification;
use praxis_app_gateway_protocol::ThreadRealtimeItemAddedNotification;
use praxis_app_gateway_protocol::ThreadRealtimeOutputAudioDeltaNotification;
use praxis_app_gateway_protocol::ThreadRealtimeStartedNotification;
use praxis_app_gateway_protocol::ThreadRealtimeTranscriptUpdatedNotification;
use praxis_protocol::ThreadId;
use praxis_protocol::protocol::RealtimeConversationClosedEvent;
use praxis_protocol::protocol::RealtimeConversationStartedEvent;
use praxis_protocol::protocol::RealtimeEvent;

pub(crate) async fn send_realtime_started(
    outgoing: &ThreadScopedOutgoingMessageSender,
    thread_id: &ThreadId,
    event: RealtimeConversationStartedEvent,
) {
    let notification = ThreadRealtimeStartedNotification {
        thread_id: thread_id.to_string(),
        session_id: event.session_id,
        version: event.version,
    };
    outgoing
        .send_server_notification(ServerNotification::ThreadRealtimeStarted(notification))
        .await;
}

pub(crate) async fn send_realtime_event(
    outgoing: &ThreadScopedOutgoingMessageSender,
    thread_id: &ThreadId,
    event: RealtimeEvent,
) {
    let thread_id = thread_id.to_string();
    match event {
        RealtimeEvent::SessionUpdated { .. } => {}
        RealtimeEvent::InputAudioSpeechStarted(event) => {
            send_realtime_item_added(
                outgoing,
                thread_id,
                serde_json::json!({
                    "type": "input_audio_buffer.speech_started",
                    "item_id": event.item_id,
                }),
            )
            .await;
        }
        RealtimeEvent::InputTranscriptDelta(event) => {
            send_realtime_transcript_delta(outgoing, thread_id, "user", event.delta).await;
        }
        RealtimeEvent::OutputTranscriptDelta(event) => {
            send_realtime_transcript_delta(outgoing, thread_id, "assistant", event.delta).await;
        }
        RealtimeEvent::AudioOut(audio) => {
            let notification = ThreadRealtimeOutputAudioDeltaNotification {
                thread_id,
                audio: audio.into(),
            };
            outgoing
                .send_server_notification(ServerNotification::ThreadRealtimeOutputAudioDelta(
                    notification,
                ))
                .await;
        }
        RealtimeEvent::ResponseCancelled(event) => {
            send_realtime_item_added(
                outgoing,
                thread_id,
                serde_json::json!({
                    "type": "response.cancelled",
                    "response_id": event.response_id,
                }),
            )
            .await;
        }
        RealtimeEvent::ConversationItemAdded(item) => {
            send_realtime_item_added(outgoing, thread_id, item).await;
        }
        RealtimeEvent::ConversationItemDone { .. } => {}
        RealtimeEvent::HandoffRequested(handoff) => {
            send_realtime_item_added(
                outgoing,
                thread_id,
                serde_json::json!({
                    "type": "handoff_request",
                    "handoff_id": handoff.handoff_id,
                    "item_id": handoff.item_id,
                    "input_transcript": handoff.input_transcript,
                    "active_transcript": handoff.active_transcript,
                }),
            )
            .await;
        }
        RealtimeEvent::Error(message) => {
            let notification = ThreadRealtimeErrorNotification { thread_id, message };
            outgoing
                .send_server_notification(ServerNotification::ThreadRealtimeError(notification))
                .await;
        }
    }
}

pub(crate) async fn send_realtime_closed(
    outgoing: &ThreadScopedOutgoingMessageSender,
    thread_id: &ThreadId,
    event: RealtimeConversationClosedEvent,
) {
    let notification = ThreadRealtimeClosedNotification {
        thread_id: thread_id.to_string(),
        reason: event.reason,
    };
    outgoing
        .send_server_notification(ServerNotification::ThreadRealtimeClosed(notification))
        .await;
}

async fn send_realtime_item_added(
    outgoing: &ThreadScopedOutgoingMessageSender,
    thread_id: String,
    item: serde_json::Value,
) {
    let notification = ThreadRealtimeItemAddedNotification { thread_id, item };
    outgoing
        .send_server_notification(ServerNotification::ThreadRealtimeItemAdded(notification))
        .await;
}

async fn send_realtime_transcript_delta(
    outgoing: &ThreadScopedOutgoingMessageSender,
    thread_id: String,
    role: &str,
    text: String,
) {
    let notification = ThreadRealtimeTranscriptUpdatedNotification {
        thread_id,
        role: role.to_string(),
        text,
    };
    outgoing
        .send_server_notification(ServerNotification::ThreadRealtimeTranscriptUpdated(
            notification,
        ))
        .await;
}
