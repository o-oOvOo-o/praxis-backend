use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;
use std::sync::atomic::AtomicI64;
use std::sync::atomic::Ordering;

use praxis_app_gateway_protocol::JSONRPCErrorError;
use praxis_app_gateway_protocol::RequestId;
use praxis_app_gateway_protocol::Result;
use praxis_app_gateway_protocol::ServerNotification;
use praxis_app_gateway_protocol::ServerRequest;
use praxis_app_gateway_protocol::ServerRequestPayload;
use praxis_otel::span_w3c_trace_context;
use praxis_protocol::ThreadId;
use praxis_protocol::protocol::W3cTraceContext;
use serde::Serialize;
use tokio::sync::Mutex;
use tokio::sync::mpsc;
use tokio::sync::oneshot;
use tracing::Instrument;
use tracing::Span;
use tracing::warn;

use crate::error_code::INTERNAL_ERROR_CODE;
use crate::server_request_callbacks::ClientResponseDisposition;
use crate::server_request_callbacks::ResponseConnectionScope;
use crate::server_request_callbacks::ServerRequestCallbackRegistry;
use crate::server_request_callbacks::controller_connection_closed_error;
use crate::server_request_error::TURN_TRANSITION_PENDING_REQUEST_ERROR_REASON;

pub(crate) type ClientRequestResult = std::result::Result<Result, JSONRPCErrorError>;

/// Stable identifier for a transport connection.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct ConnectionId(pub(crate) u64);

impl fmt::Display for ConnectionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Stable identifier for a client request scoped to a transport connection.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct ConnectionRequestId {
    pub(crate) connection_id: ConnectionId,
    pub(crate) request_id: RequestId,
}

impl ConnectionRequestId {
    pub(crate) fn new(connection_id: ConnectionId, request_id: RequestId) -> Self {
        Self {
            connection_id,
            request_id,
        }
    }
}

/// Trace data we keep for an incoming request until we send its final
/// response or error.
#[derive(Clone)]
pub(crate) struct RequestContext {
    request_id: ConnectionRequestId,
    span: Span,
    parent_trace: Option<W3cTraceContext>,
}

impl RequestContext {
    pub(crate) fn new(
        request_id: ConnectionRequestId,
        span: Span,
        parent_trace: Option<W3cTraceContext>,
    ) -> Self {
        Self {
            request_id,
            span,
            parent_trace,
        }
    }

    pub(crate) fn request_trace(&self) -> Option<W3cTraceContext> {
        span_w3c_trace_context(&self.span).or_else(|| self.parent_trace.clone())
    }

    pub(crate) fn span(&self) -> Span {
        self.span.clone()
    }

    fn record_turn_id(&self, turn_id: &str) {
        self.span.record("turn.id", turn_id);
    }
}

#[derive(Debug)]
pub(crate) enum OutgoingEnvelope {
    ToConnection {
        connection_id: ConnectionId,
        message: OutgoingMessage,
        write_complete_tx: Option<oneshot::Sender<()>>,
    },
    Broadcast {
        message: OutgoingMessage,
    },
}

#[derive(Debug)]
pub(crate) struct QueuedOutgoingMessage {
    pub(crate) message: OutgoingMessage,
    pub(crate) write_complete_tx: Option<oneshot::Sender<()>>,
}

impl QueuedOutgoingMessage {
    pub(crate) fn new(message: OutgoingMessage) -> Self {
        Self {
            message,
            write_complete_tx: None,
        }
    }
}

/// Sends messages to the client and manages request callbacks.
pub(crate) struct OutgoingMessageSender {
    next_server_request_id: AtomicI64,
    sender: mpsc::Sender<OutgoingEnvelope>,
    server_request_callbacks: ServerRequestCallbackRegistry,
    /// Incoming requests that are still waiting on a final response or error.
    /// We keep them here because this is where responses, errors, and
    /// disconnect cleanup all get handled.
    request_contexts: Mutex<HashMap<ConnectionRequestId, RequestContext>>,
}

#[derive(Clone)]
pub(crate) struct ThreadScopedOutgoingMessageSender {
    outgoing: Arc<OutgoingMessageSender>,
    connection_ids: Arc<Vec<ConnectionId>>,
    thread_id: ThreadId,
}

impl ThreadScopedOutgoingMessageSender {
    pub(crate) fn new(
        outgoing: Arc<OutgoingMessageSender>,
        connection_ids: Vec<ConnectionId>,
        thread_id: ThreadId,
    ) -> Self {
        Self {
            outgoing,
            connection_ids: Arc::new(connection_ids),
            thread_id,
        }
    }

    #[cfg(test)]
    pub(crate) async fn send_request(
        &self,
        payload: ServerRequestPayload,
    ) -> (RequestId, oneshot::Receiver<ClientRequestResult>) {
        self.outgoing
            .send_request_to_connections(
                Some(self.connection_ids.as_slice()),
                payload,
                Some(self.thread_id),
                ResponseConnectionScope::connections(self.connection_ids.iter().copied()),
            )
            .await
    }

    pub(crate) fn thread_id(&self) -> ThreadId {
        self.thread_id
    }

    pub(crate) fn outgoing_sender(&self) -> Arc<OutgoingMessageSender> {
        Arc::clone(&self.outgoing)
    }

    pub(crate) async fn register_request(
        &self,
        payload: ServerRequestPayload,
        response_connection_id: Option<ConnectionId>,
    ) -> (
        RequestId,
        oneshot::Receiver<ClientRequestResult>,
        ServerRequest,
    ) {
        self.outgoing
            .register_request_callback(
                payload,
                Some(self.thread_id),
                ResponseConnectionScope::connections(response_connection_id),
            )
            .await
    }

    pub(crate) async fn send_registered_request_to_connection(
        &self,
        connection_id: ConnectionId,
        request: ServerRequest,
    ) -> bool {
        self.outgoing
            .send_registered_request_to_connections(Some(&[connection_id]), request)
            .await
    }

    pub(crate) async fn fail_request(&self, id: &RequestId, error: JSONRPCErrorError) -> bool {
        self.outgoing.fail_request(id, error).await
    }

    pub(crate) async fn send_server_notification(&self, notification: ServerNotification) {
        if self.connection_ids.is_empty() {
            return;
        }
        self.outgoing
            .send_server_notification_to_connections(self.connection_ids.as_slice(), notification)
            .await;
    }

    pub(crate) async fn send_global_server_notification(&self, notification: ServerNotification) {
        self.outgoing.send_server_notification(notification).await;
    }

    pub(crate) async fn abort_pending_server_requests(&self) {
        self.outgoing
            .cancel_requests_for_thread(
                self.thread_id,
                Some(JSONRPCErrorError {
                    code: INTERNAL_ERROR_CODE,
                    message: "client request resolved because the turn state was changed"
                        .to_string(),
                    data: Some(serde_json::json!({ "reason": TURN_TRANSITION_PENDING_REQUEST_ERROR_REASON })),
                }),
            )
            .await
    }

    pub(crate) async fn send_response<T: Serialize>(
        &self,
        request_id: ConnectionRequestId,
        response: T,
    ) {
        self.outgoing.send_response(request_id, response).await;
    }

    pub(crate) async fn send_error(
        &self,
        request_id: ConnectionRequestId,
        error: JSONRPCErrorError,
    ) {
        self.outgoing.send_error(request_id, error).await;
    }
}

impl OutgoingMessageSender {
    pub(crate) fn new(sender: mpsc::Sender<OutgoingEnvelope>) -> Self {
        Self {
            next_server_request_id: AtomicI64::new(0),
            sender,
            server_request_callbacks: ServerRequestCallbackRegistry::default(),
            request_contexts: Mutex::new(HashMap::new()),
        }
    }

    pub(crate) async fn register_request_context(&self, request_context: RequestContext) {
        let mut request_contexts = self.request_contexts.lock().await;
        if request_contexts
            .insert(request_context.request_id.clone(), request_context)
            .is_some()
        {
            warn!("replaced unresolved request context");
        }
    }

    pub(crate) async fn connection_closed(&self, connection_id: ConnectionId) {
        {
            let mut request_contexts = self.request_contexts.lock().await;
            request_contexts.retain(|request_id, _| request_id.connection_id != connection_id);
        }
        let failed_request_ids = self
            .server_request_callbacks
            .fail_connection(connection_id, controller_connection_closed_error())
            .await;
        for request_id in failed_request_ids {
            warn!(
                ?request_id,
                ?connection_id,
                "cancelled server request after controlling connection closed"
            );
        }
    }

    pub(crate) async fn request_trace_context(
        &self,
        request_id: &ConnectionRequestId,
    ) -> Option<W3cTraceContext> {
        let request_contexts = self.request_contexts.lock().await;
        request_contexts
            .get(request_id)
            .and_then(RequestContext::request_trace)
    }

    pub(crate) async fn record_request_turn_id(
        &self,
        request_id: &ConnectionRequestId,
        turn_id: &str,
    ) {
        let request_contexts = self.request_contexts.lock().await;
        if let Some(request_context) = request_contexts.get(request_id) {
            request_context.record_turn_id(turn_id);
        }
    }

    async fn take_request_context(
        &self,
        request_id: &ConnectionRequestId,
    ) -> Option<RequestContext> {
        let mut request_contexts = self.request_contexts.lock().await;
        request_contexts.remove(request_id)
    }

    #[cfg(test)]
    async fn request_context_count(&self) -> usize {
        self.request_contexts.lock().await.len()
    }

    pub(crate) async fn send_request(
        &self,
        request: ServerRequestPayload,
    ) -> (RequestId, oneshot::Receiver<ClientRequestResult>) {
        self.send_request_to_connections(
            /*connection_ids*/ None,
            request,
            /*thread_id*/ None,
            ResponseConnectionScope::Any,
        )
        .await
    }

    fn next_request_id(&self) -> RequestId {
        RequestId::Integer(self.next_server_request_id.fetch_add(1, Ordering::Relaxed))
    }

    async fn send_request_to_connections(
        &self,
        connection_ids: Option<&[ConnectionId]>,
        request: ServerRequestPayload,
        thread_id: Option<ThreadId>,
        response_scope: ResponseConnectionScope,
    ) -> (RequestId, oneshot::Receiver<ClientRequestResult>) {
        let (request_id, receiver, request) = self
            .register_request_callback(request, thread_id, response_scope)
            .await;
        if !self
            .send_registered_request_to_connections(connection_ids, request)
            .await
        {
            self.cancel_request(&request_id).await;
        }
        (request_id, receiver)
    }

    async fn register_request_callback(
        &self,
        payload: ServerRequestPayload,
        thread_id: Option<ThreadId>,
        response_scope: ResponseConnectionScope,
    ) -> (
        RequestId,
        oneshot::Receiver<ClientRequestResult>,
        ServerRequest,
    ) {
        let request_id = self.next_request_id();
        let request = payload.request_with_id(request_id.clone());

        let (tx_approve, rx_approve) = oneshot::channel();
        self.server_request_callbacks
            .insert(request.clone(), tx_approve, thread_id, response_scope)
            .await;

        (request_id, rx_approve, request)
    }

    async fn send_registered_request_to_connections(
        &self,
        connection_ids: Option<&[ConnectionId]>,
        request: ServerRequest,
    ) -> bool {
        let outgoing_message_id = request.id().clone();
        if connection_ids.is_some_and(<[ConnectionId]>::is_empty) {
            warn!(
                ?outgoing_message_id,
                "refusing to send server request without a controlling connection"
            );
            return false;
        }
        let outgoing_message = OutgoingMessage::Request(request);
        let send_result = match connection_ids {
            None => {
                self.sender
                    .send(OutgoingEnvelope::Broadcast {
                        message: outgoing_message,
                    })
                    .await
            }
            Some(connection_ids) => {
                let mut send_error = None;
                for connection_id in connection_ids {
                    if let Err(err) = self
                        .sender
                        .send(OutgoingEnvelope::ToConnection {
                            connection_id: *connection_id,
                            message: outgoing_message.clone(),
                            write_complete_tx: None,
                        })
                        .await
                    {
                        send_error = Some(err);
                        break;
                    }
                }
                match send_error {
                    Some(err) => Err(err),
                    None => Ok(()),
                }
            }
        };

        if let Err(err) = send_result {
            warn!("failed to send request {outgoing_message_id:?} to client: {err:?}");
            return false;
        }

        true
    }

    pub(crate) async fn replay_requests_to_connection_for_thread(
        &self,
        connection_id: ConnectionId,
        requests: Vec<ServerRequest>,
    ) {
        for request in requests {
            if !self
                .server_request_callbacks
                .is_response_allowed(request.id(), connection_id)
                .await
            {
                continue;
            }
            if let Err(err) = self
                .sender
                .send(OutgoingEnvelope::ToConnection {
                    connection_id,
                    message: OutgoingMessage::Request(request),
                    write_complete_tx: None,
                })
                .await
            {
                warn!("failed to resend request to client: {err:?}");
            }
        }
    }

    pub(crate) async fn notify_client_response(
        &self,
        connection_id: ConnectionId,
        id: RequestId,
        result: Result,
    ) {
        let disposition = self
            .server_request_callbacks
            .notify_response(connection_id, &id, result)
            .await;
        Self::log_client_response_disposition(connection_id, &id, disposition);
    }

    pub(crate) async fn notify_client_error(
        &self,
        connection_id: ConnectionId,
        id: RequestId,
        error: JSONRPCErrorError,
    ) {
        let disposition = self
            .server_request_callbacks
            .notify_error(connection_id, &id, error)
            .await;
        Self::log_client_response_disposition(connection_id, &id, disposition);
    }

    fn log_client_response_disposition(
        connection_id: ConnectionId,
        request_id: &RequestId,
        disposition: ClientResponseDisposition,
    ) {
        match disposition {
            ClientResponseDisposition::Delivered => {}
            ClientResponseDisposition::UnknownRequest => {
                warn!(
                    ?request_id,
                    ?connection_id,
                    "could not find server request callback"
                );
            }
            ClientResponseDisposition::WrongConnection => {
                warn!(
                    ?request_id,
                    ?connection_id,
                    "rejected server request response from non-controlling connection"
                );
            }
            ClientResponseDisposition::WaiterDropped => {
                warn!(
                    ?request_id,
                    ?connection_id,
                    "server request waiter was dropped"
                );
            }
        }
    }

    pub(crate) async fn cancel_request(&self, id: &RequestId) -> bool {
        self.server_request_callbacks.cancel(id).await
    }

    pub(crate) async fn fail_request(&self, id: &RequestId, error: JSONRPCErrorError) -> bool {
        self.server_request_callbacks.fail(id, error).await
    }

    pub(crate) async fn cancel_all_requests(&self, error: Option<JSONRPCErrorError>) {
        self.server_request_callbacks.fail_all(error).await;
    }

    pub(crate) async fn pending_requests_for_thread(
        &self,
        thread_id: ThreadId,
    ) -> Vec<ServerRequest> {
        self.server_request_callbacks
            .pending_requests_for_thread(thread_id)
            .await
    }

    pub(crate) async fn cancel_requests_for_thread(
        &self,
        thread_id: ThreadId,
        error: Option<JSONRPCErrorError>,
    ) {
        self.server_request_callbacks
            .fail_thread(thread_id, error)
            .await;
    }

    pub(crate) async fn resolve_pending_approval_requests(
        &self,
        thread_id: ThreadId,
        error: JSONRPCErrorError,
    ) {
        let requests = self.pending_requests_for_thread(thread_id).await;
        for request in requests {
            if praxis_app_gateway_protocol::is_approval_server_request(&request) {
                self.fail_request(request.id(), error.clone()).await;
            }
        }
    }

    pub(crate) async fn send_response<T: Serialize>(
        &self,
        request_id: ConnectionRequestId,
        response: T,
    ) {
        let request_context = self.take_request_context(&request_id).await;
        match serde_json::to_value(response) {
            Ok(result) => {
                let outgoing_message = OutgoingMessage::Response(OutgoingResponse {
                    id: request_id.request_id.clone(),
                    result,
                });
                self.send_outgoing_message_to_connection(
                    request_context,
                    request_id.connection_id,
                    outgoing_message,
                    "response",
                )
                .await;
            }
            Err(err) => {
                self.send_error_inner(
                    request_context,
                    request_id,
                    JSONRPCErrorError {
                        code: INTERNAL_ERROR_CODE,
                        message: format!("failed to serialize response: {err}"),
                        data: None,
                    },
                )
                .await;
            }
        }
    }

    pub(crate) async fn send_server_notification(&self, notification: ServerNotification) {
        self.send_server_notification_to_connections(&[], notification)
            .await;
    }

    pub(crate) async fn send_server_notification_to_connections(
        &self,
        connection_ids: &[ConnectionId],
        notification: ServerNotification,
    ) {
        tracing::trace!(
            targeted_connections = connection_ids.len(),
            "app-gateway event: {notification}"
        );
        let outgoing_message = OutgoingMessage::AppGatewayNotification(notification);
        if connection_ids.is_empty() {
            if let Err(err) = self
                .sender
                .send(OutgoingEnvelope::Broadcast {
                    message: outgoing_message,
                })
                .await
            {
                warn!("failed to send server notification to client: {err:?}");
            }
            return;
        }
        for connection_id in connection_ids {
            if let Err(err) = self
                .sender
                .send(OutgoingEnvelope::ToConnection {
                    connection_id: *connection_id,
                    message: outgoing_message.clone(),
                    write_complete_tx: None,
                })
                .await
            {
                warn!("failed to send server notification to client: {err:?}");
            }
        }
    }

    pub(crate) async fn send_server_notification_to_connection_and_wait(
        &self,
        connection_id: ConnectionId,
        notification: ServerNotification,
    ) {
        tracing::trace!("app-gateway event: {notification}");
        let outgoing_message = OutgoingMessage::AppGatewayNotification(notification);
        let (write_complete_tx, write_complete_rx) = oneshot::channel();
        if let Err(err) = self
            .sender
            .send(OutgoingEnvelope::ToConnection {
                connection_id,
                message: outgoing_message,
                write_complete_tx: Some(write_complete_tx),
            })
            .await
        {
            warn!("failed to send server notification to client: {err:?}");
        }
        let _ = write_complete_rx.await;
    }

    pub(crate) async fn send_error(
        &self,
        request_id: ConnectionRequestId,
        error: JSONRPCErrorError,
    ) {
        let request_context = self.take_request_context(&request_id).await;
        self.send_error_inner(request_context, request_id, error)
            .await;
    }

    async fn send_error_inner(
        &self,
        request_context: Option<RequestContext>,
        request_id: ConnectionRequestId,
        error: JSONRPCErrorError,
    ) {
        let outgoing_message = OutgoingMessage::Error(OutgoingError {
            id: request_id.request_id,
            error,
        });
        self.send_outgoing_message_to_connection(
            request_context,
            request_id.connection_id,
            outgoing_message,
            "error",
        )
        .await;
    }

    async fn send_outgoing_message_to_connection(
        &self,
        request_context: Option<RequestContext>,
        connection_id: ConnectionId,
        message: OutgoingMessage,
        message_kind: &'static str,
    ) {
        let send_fut = self.sender.send(OutgoingEnvelope::ToConnection {
            connection_id,
            message,
            write_complete_tx: None,
        });
        let send_result = if let Some(request_context) = request_context {
            send_fut.instrument(request_context.span()).await
        } else {
            send_fut.await
        };

        if let Err(err) = send_result {
            warn!("failed to send {message_kind} to client: {err:?}");
        }
    }
}

/// Outgoing message from the server to the client.
#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub(crate) enum OutgoingMessage {
    Request(ServerRequest),
    /// AppGatewayNotification is specific to the case where this is run as an
    /// "app gateway" as opposed to an MCP server.
    AppGatewayNotification(ServerNotification),
    Response(OutgoingResponse),
    Error(OutgoingError),
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct OutgoingResponse {
    pub id: RequestId,
    pub result: Result,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct OutgoingError {
    pub error: JSONRPCErrorError,
    pub id: RequestId,
}

#[cfg(test)]
mod tests;
