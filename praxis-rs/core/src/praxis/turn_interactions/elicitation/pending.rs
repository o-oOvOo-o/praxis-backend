use praxis_rmcp_client::ElicitationResponse;
use rmcp::model::RequestId;
use tokio::sync::oneshot;
use tracing::warn;

use crate::praxis::Session;

pub(super) async fn insert_pending_elicitation(
    session: &Session,
    server_name: String,
    request_id: RequestId,
    tx_response: oneshot::Sender<ElicitationResponse>,
) {
    let prev_entry = {
        let mut active = session.active_turn.lock().await;
        match active.as_mut() {
            Some(at) => {
                let mut ts = at.turn_state.lock().await;
                ts.insert_pending_elicitation(server_name.clone(), request_id.clone(), tx_response)
            }
            None => None,
        }
    };
    if prev_entry.is_some() {
        warn!(
            "Overwriting existing pending elicitation for server_name: {server_name}, request_id: {request_id}"
        );
    }
}

pub(super) async fn remove_pending_elicitation(
    session: &Session,
    server_name: &str,
    id: &RequestId,
) -> Option<oneshot::Sender<ElicitationResponse>> {
    let mut active = session.active_turn.lock().await;
    match active.as_mut() {
        Some(at) => {
            let mut ts = at.turn_state.lock().await;
            ts.remove_pending_elicitation(server_name, id)
        }
        None => None,
    }
}
