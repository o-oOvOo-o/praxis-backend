use crate::client_response_decode::PendingClientResponse;
use crate::outgoing_message::ClientRequestResult;
use crate::outgoing_message::ThreadScopedOutgoingMessageSender;
use crate::thread_state::ThreadListenerCommand;
use crate::thread_state::ThreadState;
use crate::thread_status::ThreadWatchActiveGuard;
use praxis_app_gateway_protocol::RequestId;
use praxis_app_gateway_protocol::ServerRequestPayload;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::sync::oneshot;
use tracing::error;

pub(crate) struct PendingServerRequest {
    request_id: RequestId,
    receiver: oneshot::Receiver<ClientRequestResult>,
}

pub(crate) async fn send_server_request(
    outgoing: &ThreadScopedOutgoingMessageSender,
    payload: ServerRequestPayload,
) -> PendingServerRequest {
    let (request_id, receiver) = outgoing.send_request(payload).await;
    PendingServerRequest {
        request_id,
        receiver,
    }
}

impl PendingServerRequest {
    pub(crate) async fn await_response_and_resolve(
        self,
        thread_state: &Arc<Mutex<ThreadState>>,
        guard: ThreadWatchActiveGuard,
    ) -> PendingClientResponse {
        let response = self.receiver.await;
        resolve_server_request_on_thread_listener(thread_state, self.request_id).await;
        drop(guard);
        response
    }
}

pub(crate) async fn resolve_server_request_on_thread_listener(
    thread_state: &Arc<Mutex<ThreadState>>,
    request_id: RequestId,
) {
    let (completion_tx, completion_rx) = oneshot::channel();
    let listener_command_tx = {
        let state = thread_state.lock().await;
        state.listener_command_tx()
    };
    let Some(listener_command_tx) = listener_command_tx else {
        error!("failed to remove pending client request: thread listener is not running");
        return;
    };

    if listener_command_tx
        .send(ThreadListenerCommand::ResolveServerRequest {
            request_id,
            completion_tx,
        })
        .is_err()
    {
        error!(
            "failed to remove pending client request: thread listener command channel is closed"
        );
        return;
    }

    if let Err(err) = completion_rx.await {
        error!("failed to remove pending client request: {err}");
    }
}
