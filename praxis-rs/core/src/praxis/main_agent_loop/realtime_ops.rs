use std::sync::Arc;

use praxis_protocol::protocol::ConversationAudioParams;
use praxis_protocol::protocol::ConversationStartParams;
use praxis_protocol::protocol::ConversationTextParams;
use praxis_protocol::protocol::PraxisErrorInfo;

use crate::praxis::Session;
use crate::realtime_conversation::handle_audio as handle_realtime_conversation_audio;
use crate::realtime_conversation::handle_close as handle_realtime_conversation_close;
use crate::realtime_conversation::handle_start as handle_realtime_conversation_start;
use crate::realtime_conversation::handle_text as handle_realtime_conversation_text;

pub(super) async fn start(sess: &Arc<Session>, sub_id: String, params: ConversationStartParams) {
    if let Err(err) = handle_realtime_conversation_start(sess, sub_id.clone(), params).await {
        sess.raw_event_emitter(sub_id)
            .error(err.to_string(), Some(PraxisErrorInfo::Other))
            .await;
    }
}

pub(super) async fn audio(sess: &Arc<Session>, sub_id: String, params: ConversationAudioParams) {
    handle_realtime_conversation_audio(sess, sub_id, params).await;
}

pub(super) async fn text(sess: &Arc<Session>, sub_id: String, params: ConversationTextParams) {
    handle_realtime_conversation_text(sess, sub_id, params).await;
}

pub(super) async fn close(sess: &Arc<Session>, sub_id: String) {
    handle_realtime_conversation_close(sess, sub_id).await;
}
