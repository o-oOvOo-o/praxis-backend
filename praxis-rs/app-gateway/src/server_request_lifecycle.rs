use crate::client_response_decode::PendingClientResponse;
use crate::error_code::INTERNAL_ERROR_CODE;
use crate::outgoing_message::ClientRequestResult;
use crate::outgoing_message::OutgoingMessageSender;
use crate::outgoing_message::ThreadScopedOutgoingMessageSender;
use crate::thread_state::ThreadListenerCommand;
use crate::thread_state::ThreadState;
use crate::thread_state::ThreadStateManager;
use praxis_app_gateway_protocol::JSONRPCErrorError;
use praxis_app_gateway_protocol::RequestId;
use praxis_app_gateway_protocol::ServerNotification;
use praxis_app_gateway_protocol::ServerRequestPayload;
use praxis_app_gateway_protocol::ServerRequestResolvedNotification;
use praxis_protocol::ThreadId;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::sync::oneshot;
use tracing::error;

pub(crate) struct PendingServerRequest {
    request_id: RequestId,
    receiver: oneshot::Receiver<ClientRequestResult>,
    thread_id: ThreadId,
    thread_state_manager: ThreadStateManager,
    outgoing: Arc<OutgoingMessageSender>,
}

pub(crate) async fn send_server_request(
    thread_state_manager: &ThreadStateManager,
    thread_state: &Arc<Mutex<ThreadState>>,
    outgoing: &ThreadScopedOutgoingMessageSender,
    turn_id: &str,
    payload: ServerRequestPayload,
) -> PendingServerRequest {
    let thread_id = outgoing.thread_id();
    let response_connection_id = thread_state_manager
        .turn_controller(thread_id, turn_id)
        .await;
    let (request_id, receiver, request) = outgoing
        .register_request(payload, response_connection_id)
        .await;
    {
        let mut state = thread_state.lock().await;
        state.insert_pending_server_request(request.clone());
    }

    let sent = match response_connection_id {
        Some(connection_id) => {
            outgoing
                .send_registered_request_to_connection(connection_id, request)
                .await
        }
        None => false,
    };
    if !sent {
        outgoing
            .fail_request(
                &request_id,
                JSONRPCErrorError {
                    code: INTERNAL_ERROR_CODE,
                    message: "server request has no live controlling connection".to_string(),
                    data: Some(serde_json::json!({
                        "reason": "missingTurnController",
                    })),
                },
            )
            .await;
        thread_state
            .lock()
            .await
            .remove_pending_server_request(&request_id);
    }

    PendingServerRequest {
        request_id,
        receiver,
        thread_id,
        thread_state_manager: thread_state_manager.clone(),
        outgoing: outgoing.outgoing_sender(),
    }
}

impl PendingServerRequest {
    pub(crate) async fn await_response_and_resolve(
        self,
        thread_state: &Arc<Mutex<ThreadState>>,
    ) -> PendingClientResponse {
        let response = self.receiver.await;
        resolve_server_request_on_thread_listener(
            &self.thread_state_manager,
            &self.outgoing,
            thread_state,
            self.thread_id,
            self.request_id,
        )
        .await;
        response
    }
}

pub(crate) async fn resolve_server_request_on_thread_listener(
    thread_state_manager: &ThreadStateManager,
    outgoing: &Arc<OutgoingMessageSender>,
    thread_state: &Arc<Mutex<ThreadState>>,
    thread_id: ThreadId,
    request_id: RequestId,
) {
    let (completion_tx, completion_rx) = oneshot::channel();
    let listener_command_tx = {
        let state = thread_state.lock().await;
        state.listener_command_tx()
    };
    let Some(listener_command_tx) = listener_command_tx else {
        error!("failed to remove pending client request: thread listener is not running");
        resolve_pending_server_request(
            thread_id,
            thread_state_manager,
            outgoing,
            thread_state,
            request_id,
        )
        .await;
        return;
    };

    if listener_command_tx
        .send(ThreadListenerCommand::ResolveServerRequest {
            request_id: request_id.clone(),
            completion_tx,
        })
        .await
        .is_err()
    {
        error!(
            "failed to remove pending client request: thread listener command channel is closed"
        );
        resolve_pending_server_request(
            thread_id,
            thread_state_manager,
            outgoing,
            thread_state,
            request_id,
        )
        .await;
        return;
    }

    if let Err(err) = completion_rx.await {
        error!("failed to remove pending client request: {err}");
    }
}

pub(crate) async fn resolve_pending_server_request(
    thread_id: ThreadId,
    thread_state_manager: &ThreadStateManager,
    outgoing: &Arc<OutgoingMessageSender>,
    thread_state: &Arc<Mutex<ThreadState>>,
    request_id: RequestId,
) {
    let removed = {
        let mut state = thread_state.lock().await;
        state.remove_pending_server_request(&request_id)
    };
    if !removed {
        return;
    }
    let subscribed_connection_ids = thread_state_manager
        .subscribed_connection_ids(thread_id)
        .await;
    let outgoing = ThreadScopedOutgoingMessageSender::new(
        outgoing.clone(),
        subscribed_connection_ids,
        thread_id,
    );
    outgoing
        .send_server_notification(ServerNotification::ServerRequestResolved(
            ServerRequestResolvedNotification {
                thread_id: thread_id.to_string(),
                request_id,
            },
        ))
        .await;
}
