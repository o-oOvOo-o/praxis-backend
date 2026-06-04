/*
This module implements the websocket-backed app-gateway client transport.

It owns the remote connection lifecycle, including the initialize/initialized
handshake, JSON-RPC request/response routing, server-request resolution, and
notification streaming. The rest of the crate uses the same `AppGatewayEvent`
surface for both in-process and remote transports, so callers such as the TUI
can switch between them without changing their higher-level session logic.
*/

use std::collections::HashMap;
use std::collections::VecDeque;
use std::io::Error as IoError;
use std::io::ErrorKind;
use std::io::Result as IoResult;
use std::time::Duration;

use crate::AppGatewayClientCommand;
use crate::AppGatewayCommandEndpoint;
use crate::AppGatewayEvent;
use crate::CommandEndpointLabels;
use crate::RequestResult;
use crate::SHUTDOWN_TIMEOUT;
use crate::TypedRequestError;
use crate::initialize_params_from_metadata;
use crate::server_notification_requires_delivery;
use futures::SinkExt;
use futures::StreamExt;
use praxis_app_gateway_protocol::ClientNotification;
use praxis_app_gateway_protocol::ClientRequest;
use praxis_app_gateway_protocol::InitializeParams;
use praxis_app_gateway_protocol::JSONRPCError;
use praxis_app_gateway_protocol::JSONRPCErrorError;
use praxis_app_gateway_protocol::JSONRPCMessage;
use praxis_app_gateway_protocol::JSONRPCNotification;
use praxis_app_gateway_protocol::JSONRPCRequest;
use praxis_app_gateway_protocol::JSONRPCResponse;
use praxis_app_gateway_protocol::RequestId;
use praxis_app_gateway_protocol::Result as JsonRpcResult;
use praxis_app_gateway_protocol::ServerNotification;
use praxis_app_gateway_protocol::ServerRequest;
use serde::de::DeserializeOwned;
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio::sync::oneshot;
use tokio::time::timeout;
use tokio_tungstenite::MaybeTlsStream;
use tokio_tungstenite::WebSocketStream;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::HeaderValue;
use tokio_tungstenite::tungstenite::http::header::AUTHORIZATION;
use tracing::warn;
use url::Url;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const INITIALIZE_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Clone)]
pub struct RemoteAppGatewayConnectArgs {
    pub websocket_url: String,
    pub auth_token: Option<String>,
    pub client_name: String,
    pub client_version: String,
    pub experimental_api: bool,
    pub opt_out_notification_methods: Vec<String>,
    pub channel_capacity: usize,
}

impl RemoteAppGatewayConnectArgs {
    fn initialize_params(&self) -> InitializeParams {
        initialize_params_from_metadata(
            self.client_name.as_str(),
            self.client_version.as_str(),
            self.experimental_api,
            &self.opt_out_notification_methods,
        )
    }
}

pub(crate) fn websocket_url_supports_auth_token(url: &Url) -> bool {
    match (url.scheme(), url.host()) {
        ("wss", Some(_)) => true,
        ("ws", Some(url::Host::Domain(domain))) => domain.eq_ignore_ascii_case("localhost"),
        ("ws", Some(url::Host::Ipv4(addr))) => addr.is_loopback(),
        ("ws", Some(url::Host::Ipv6(addr))) => addr.is_loopback(),
        _ => false,
    }
}

enum RemoteClientCommand {
    Request {
        request: Box<ClientRequest>,
        response_tx: oneshot::Sender<IoResult<RequestResult>>,
    },
    Notify {
        notification: ClientNotification,
        response_tx: oneshot::Sender<IoResult<()>>,
    },
    ResolveServerRequest {
        request_id: RequestId,
        result: JsonRpcResult,
        response_tx: oneshot::Sender<IoResult<()>>,
    },
    RejectServerRequest {
        request_id: RequestId,
        error: JSONRPCErrorError,
        response_tx: oneshot::Sender<IoResult<()>>,
    },
    Shutdown {
        response_tx: oneshot::Sender<IoResult<()>>,
    },
}

impl AppGatewayClientCommand for RemoteClientCommand {
    fn request_command(
        request: Box<ClientRequest>,
        response_tx: oneshot::Sender<IoResult<RequestResult>>,
    ) -> Self {
        Self::Request {
            request,
            response_tx,
        }
    }

    fn notify_command(
        notification: ClientNotification,
        response_tx: oneshot::Sender<IoResult<()>>,
    ) -> Self {
        Self::Notify {
            notification,
            response_tx,
        }
    }

    fn resolve_server_request_command(
        request_id: RequestId,
        result: JsonRpcResult,
        response_tx: oneshot::Sender<IoResult<()>>,
    ) -> Self {
        Self::ResolveServerRequest {
            request_id,
            result,
            response_tx,
        }
    }

    fn reject_server_request_command(
        request_id: RequestId,
        error: JSONRPCErrorError,
        response_tx: oneshot::Sender<IoResult<()>>,
    ) -> Self {
        Self::RejectServerRequest {
            request_id,
            error,
            response_tx,
        }
    }
}

fn remote_command_endpoint(
    command_tx: mpsc::Sender<RemoteClientCommand>,
) -> AppGatewayCommandEndpoint<RemoteClientCommand> {
    AppGatewayCommandEndpoint::new(
        command_tx,
        CommandEndpointLabels {
            worker_closed: "remote app-gateway worker channel is closed",
            request_closed: "remote app-gateway request channel is closed",
            notify_closed: "remote app-gateway notify channel is closed",
            resolve_closed: "remote app-gateway resolve channel is closed",
            reject_closed: "remote app-gateway reject channel is closed",
        },
    )
}

pub struct RemoteAppGatewayClient {
    command_tx: mpsc::Sender<RemoteClientCommand>,
    command_endpoint: AppGatewayCommandEndpoint<RemoteClientCommand>,
    event_rx: mpsc::Receiver<AppGatewayEvent>,
    pending_events: VecDeque<AppGatewayEvent>,
    worker_handle: tokio::task::JoinHandle<()>,
}

#[derive(Clone)]
pub struct RemoteAppGatewayRequestHandle {
    command_endpoint: AppGatewayCommandEndpoint<RemoteClientCommand>,
}

impl RemoteAppGatewayClient {
    pub async fn connect(args: RemoteAppGatewayConnectArgs) -> IoResult<Self> {
        let channel_capacity = args.channel_capacity.max(1);
        let websocket_url = args.websocket_url.clone();
        let url = Url::parse(&websocket_url).map_err(|err| {
            IoError::new(
                ErrorKind::InvalidInput,
                format!("invalid websocket URL `{websocket_url}`: {err}"),
            )
        })?;
        if args.auth_token.is_some() && !websocket_url_supports_auth_token(&url) {
            return Err(IoError::new(
                ErrorKind::InvalidInput,
                format!(
                    "remote auth tokens require `wss://` or loopback `ws://` URLs; got `{websocket_url}`"
                ),
            ));
        }
        let mut request = url.as_str().into_client_request().map_err(|err| {
            IoError::new(
                ErrorKind::InvalidInput,
                format!("invalid websocket URL `{websocket_url}`: {err}"),
            )
        })?;
        if let Some(auth_token) = args.auth_token.as_deref() {
            let header_value =
                HeaderValue::from_str(&format!("Bearer {auth_token}")).map_err(|err| {
                    IoError::new(
                        ErrorKind::InvalidInput,
                        format!("invalid remote authorization header value: {err}"),
                    )
                })?;
            request.headers_mut().insert(AUTHORIZATION, header_value);
        }
        let stream = timeout(CONNECT_TIMEOUT, connect_async(request))
            .await
            .map_err(|_| {
                IoError::new(
                    ErrorKind::TimedOut,
                    format!("timed out connecting to remote app gateway at `{websocket_url}`"),
                )
            })?
            .map(|(stream, _response)| stream)
            .map_err(|err| {
                IoError::other(format!(
                    "failed to connect to remote app gateway at `{websocket_url}`: {err}"
                ))
            })?;
        let mut stream = stream;
        let pending_events = initialize_remote_connection(
            &mut stream,
            &websocket_url,
            args.initialize_params(),
            INITIALIZE_TIMEOUT,
        )
        .await?;

        let (command_tx, mut command_rx) = mpsc::channel::<RemoteClientCommand>(channel_capacity);
        let (event_tx, event_rx) = mpsc::channel::<AppGatewayEvent>(channel_capacity);
        let worker_handle = tokio::spawn(async move {
            let mut pending_requests =
                HashMap::<RequestId, oneshot::Sender<IoResult<RequestResult>>>::new();
            let mut skipped_events = 0usize;
            loop {
                tokio::select! {
                    command = command_rx.recv() => {
                        let Some(command) = command else {
                            let _ = stream.close(None).await;
                            break;
                        };
                        match command {
                            RemoteClientCommand::Request { request, response_tx } => {
                                let request_id = request_id_from_client_request(&request);
                                if pending_requests.contains_key(&request_id) {
                                    let _ = response_tx.send(Err(IoError::new(
                                        ErrorKind::InvalidInput,
                                        format!("duplicate remote app-gateway request id `{request_id}`"),
                                    )));
                                    continue;
                                }
                                pending_requests.insert(request_id.clone(), response_tx);
                                if let Err(err) = write_jsonrpc_message(
                                    &mut stream,
                                    JSONRPCMessage::Request(jsonrpc_request_from_client_request(*request)),
                                    &websocket_url,
                                )
                                .await
                                {
                                    let err_message = err.to_string();
                                    if let Some(response_tx) = pending_requests.remove(&request_id) {
                                        let _ = response_tx.send(Err(err));
                                    }
                                    let _ = deliver_event(
                                        &event_tx,
                                        &mut skipped_events,
                                        AppGatewayEvent::Disconnected {
                                            message: format!(
                                                "remote app gateway at `{websocket_url}` write failed: {err_message}"
                                            ),
                                        },
                                        &mut stream,
                                    )
                                    .await;
                                    break;
                                }
                            }
                            RemoteClientCommand::Notify { notification, response_tx } => {
                                let result = write_jsonrpc_message(
                                    &mut stream,
                                    JSONRPCMessage::Notification(
                                        jsonrpc_notification_from_client_notification(notification),
                                    ),
                                    &websocket_url,
                                )
                                .await;
                                let _ = response_tx.send(result);
                            }
                            RemoteClientCommand::ResolveServerRequest {
                                request_id,
                                result,
                                response_tx,
                            } => {
                                let result = write_jsonrpc_message(
                                    &mut stream,
                                    JSONRPCMessage::Response(JSONRPCResponse {
                                        id: request_id,
                                        result,
                                    }),
                                    &websocket_url,
                                )
                                .await;
                                let _ = response_tx.send(result);
                            }
                            RemoteClientCommand::RejectServerRequest {
                                request_id,
                                error,
                                response_tx,
                            } => {
                                let result = write_jsonrpc_message(
                                    &mut stream,
                                    JSONRPCMessage::Error(JSONRPCError {
                                        error,
                                        id: request_id,
                                    }),
                                    &websocket_url,
                                )
                                .await;
                                let _ = response_tx.send(result);
                            }
                            RemoteClientCommand::Shutdown { response_tx } => {
                                let close_result = stream.close(None).await.map_err(|err| {
                                    IoError::other(format!(
                                        "failed to close websocket app gateway `{websocket_url}`: {err}"
                                    ))
                                });
                                let _ = response_tx.send(close_result);
                                break;
                            }
                        }
                    }
                    message = stream.next() => {
                        match message {
                            Some(Ok(Message::Text(text))) => {
                                match serde_json::from_str::<JSONRPCMessage>(&text) {
                                    Ok(JSONRPCMessage::Response(response)) => {
                                        if let Some(response_tx) = pending_requests.remove(&response.id) {
                                            let _ = response_tx.send(Ok(Ok(response.result)));
                                        }
                                    }
                                    Ok(JSONRPCMessage::Error(error)) => {
                                        if let Some(response_tx) = pending_requests.remove(&error.id) {
                                            let _ = response_tx.send(Ok(Err(error.error)));
                                        }
                                    }
                                    Ok(JSONRPCMessage::Notification(notification)) => {
                                        if let Some(event) =
                                            app_gateway_event_from_notification(notification)
                                            && let Err(err) = deliver_event(
                                                &event_tx,
                                                &mut skipped_events,
                                                event,
                                                &mut stream,
                                            )
                                            .await
                                            {
                                                warn!(%err, "failed to deliver remote app-gateway event");
                                                break;
                                            }
                                    }
                                    Ok(JSONRPCMessage::Request(request)) => {
                                        let request_id = request.id.clone();
                                        let method = request.method.clone();
                                        match ServerRequest::try_from(request) {
                                            Ok(request) => {
                                                if let Err(err) = deliver_event(
                                                    &event_tx,
                                                    &mut skipped_events,
                                                    AppGatewayEvent::ServerRequest(request),
                                                    &mut stream,
                                                )
                                                .await
                                                {
                                                    warn!(%err, "failed to deliver remote app-gateway server request");
                                                    break;
                                                }
                                            }
                                            Err(err) => {
                                                warn!(%err, method, "rejecting unknown remote app-gateway request");
                                                if let Err(reject_err) = write_jsonrpc_message(
                                                    &mut stream,
                                                    JSONRPCMessage::Error(JSONRPCError {
                                                        error: JSONRPCErrorError {
                                                            code: -32601,
                                                            message: format!(
                                                                "unsupported remote app-gateway request `{method}`"
                                                            ),
                                                            data: None,
                                                        },
                                                        id: request_id,
                                                    }),
                                                    &websocket_url,
                                                )
                                                .await
                                                {
                                                    let err_message = reject_err.to_string();
                                                    let _ = deliver_event(
                                                        &event_tx,
                                                        &mut skipped_events,
                                                        AppGatewayEvent::Disconnected {
                                                            message: format!(
                                                                "remote app gateway at `{websocket_url}` write failed: {err_message}"
                                                            ),
                                                        },
                                                        &mut stream,
                                                    )
                                                    .await;
                                                    break;
                                                }
                                            }
                                        }
                                    }
                                    Err(err) => {
                                        let _ = deliver_event(
                                            &event_tx,
                                            &mut skipped_events,
                                            AppGatewayEvent::Disconnected {
                                                message: format!(
                                                    "remote app gateway at `{websocket_url}` sent invalid JSON-RPC: {err}"
                                                ),
                                            },
                                            &mut stream,
                                        )
                                        .await;
                                        break;
                                    }
                                }
                            }
                            Some(Ok(Message::Close(frame))) => {
                                let reason = frame
                                    .as_ref()
                                    .map(|frame| frame.reason.to_string())
                                    .filter(|reason| !reason.is_empty())
                                    .unwrap_or_else(|| "connection closed".to_string());
                                let _ = deliver_event(
                                    &event_tx,
                                    &mut skipped_events,
                                    AppGatewayEvent::Disconnected {
                                        message: format!(
                                            "remote app gateway at `{websocket_url}` disconnected: {reason}"
                                        ),
                                    },
                                    &mut stream,
                                )
                                .await;
                                break;
                            }
                            Some(Ok(Message::Binary(_)))
                            | Some(Ok(Message::Ping(_)))
                            | Some(Ok(Message::Pong(_)))
                            | Some(Ok(Message::Frame(_))) => {}
                            Some(Err(err)) => {
                                let _ = deliver_event(
                                    &event_tx,
                                    &mut skipped_events,
                                    AppGatewayEvent::Disconnected {
                                        message: format!(
                                            "remote app gateway at `{websocket_url}` transport failed: {err}"
                                        ),
                                    },
                                    &mut stream,
                                )
                                .await;
                                break;
                            }
                            None => {
                                let _ = deliver_event(
                                    &event_tx,
                                    &mut skipped_events,
                                    AppGatewayEvent::Disconnected {
                                        message: format!(
                                            "remote app gateway at `{websocket_url}` closed the connection"
                                        ),
                                    },
                                    &mut stream,
                                )
                                .await;
                                break;
                            }
                        }
                    }
                }
            }

            let err = IoError::new(
                ErrorKind::BrokenPipe,
                "remote app-gateway worker channel is closed",
            );
            for (_, response_tx) in pending_requests {
                let _ = response_tx.send(Err(IoError::new(err.kind(), err.to_string())));
            }
        });

        let command_endpoint = remote_command_endpoint(command_tx.clone());
        Ok(Self {
            command_tx,
            command_endpoint,
            event_rx,
            pending_events: pending_events.into(),
            worker_handle,
        })
    }

    pub fn request_handle(&self) -> RemoteAppGatewayRequestHandle {
        RemoteAppGatewayRequestHandle {
            command_endpoint: self.command_endpoint.clone(),
        }
    }

    pub async fn request(&self, request: ClientRequest) -> IoResult<RequestResult> {
        self.command_endpoint.request(request).await
    }

    pub async fn request_typed<T>(&self, request: ClientRequest) -> Result<T, TypedRequestError>
    where
        T: DeserializeOwned,
    {
        self.command_endpoint.request_typed(request).await
    }

    pub async fn notify(&self, notification: ClientNotification) -> IoResult<()> {
        self.command_endpoint.notify(notification).await
    }

    pub async fn resolve_server_request(
        &self,
        request_id: RequestId,
        result: JsonRpcResult,
    ) -> IoResult<()> {
        self.command_endpoint
            .resolve_server_request(request_id, result)
            .await
    }

    pub async fn reject_server_request(
        &self,
        request_id: RequestId,
        error: JSONRPCErrorError,
    ) -> IoResult<()> {
        self.command_endpoint
            .reject_server_request(request_id, error)
            .await
    }

    pub async fn next_event(&mut self) -> Option<AppGatewayEvent> {
        if let Some(event) = self.pending_events.pop_front() {
            return Some(event);
        }
        self.event_rx.recv().await
    }

    pub fn try_next_event(&mut self) -> Option<AppGatewayEvent> {
        if let Some(event) = self.pending_events.pop_front() {
            return Some(event);
        }
        self.event_rx.try_recv().ok()
    }

    pub async fn shutdown(self) -> IoResult<()> {
        let Self {
            command_tx,
            command_endpoint: _command_endpoint,
            event_rx,
            pending_events: _pending_events,
            worker_handle,
        } = self;
        let mut worker_handle = worker_handle;
        drop(event_rx);
        let (response_tx, response_rx) = oneshot::channel();
        if command_tx
            .send(RemoteClientCommand::Shutdown { response_tx })
            .await
            .is_ok()
            && let Ok(command_result) = timeout(SHUTDOWN_TIMEOUT, response_rx).await
        {
            command_result.map_err(|_| {
                IoError::new(
                    ErrorKind::BrokenPipe,
                    "remote app-gateway shutdown channel is closed",
                )
            })??;
        }

        if let Err(_elapsed) = timeout(SHUTDOWN_TIMEOUT, &mut worker_handle).await {
            worker_handle.abort();
            let _ = worker_handle.await;
        }
        Ok(())
    }
}

impl RemoteAppGatewayRequestHandle {
    pub async fn request(&self, request: ClientRequest) -> IoResult<RequestResult> {
        self.command_endpoint.request(request).await
    }

    pub async fn request_typed<T>(&self, request: ClientRequest) -> Result<T, TypedRequestError>
    where
        T: DeserializeOwned,
    {
        self.command_endpoint.request_typed(request).await
    }
}

async fn initialize_remote_connection(
    stream: &mut WebSocketStream<MaybeTlsStream<TcpStream>>,
    websocket_url: &str,
    params: InitializeParams,
    initialize_timeout: Duration,
) -> IoResult<Vec<AppGatewayEvent>> {
    let initialize_request_id = RequestId::String("initialize".to_string());
    let mut pending_events = Vec::new();
    write_jsonrpc_message(
        stream,
        JSONRPCMessage::Request(jsonrpc_request_from_client_request(
            ClientRequest::Initialize {
                request_id: initialize_request_id.clone(),
                params,
            },
        )),
        websocket_url,
    )
    .await?;

    timeout(initialize_timeout, async {
        loop {
            match stream.next().await {
                Some(Ok(Message::Text(text))) => {
                    let message = serde_json::from_str::<JSONRPCMessage>(&text).map_err(|err| {
                        IoError::other(format!(
                            "remote app gateway at `{websocket_url}` sent invalid initialize response: {err}"
                        ))
                    })?;
                    match message {
                        JSONRPCMessage::Response(response) if response.id == initialize_request_id => {
                            break Ok(());
                        }
                        JSONRPCMessage::Error(error) if error.id == initialize_request_id => {
                            break Err(IoError::other(format!(
                                "remote app gateway at `{websocket_url}` rejected initialize: {}",
                                error.error.message
                            )));
                        }
                        JSONRPCMessage::Notification(notification) => {
                            if let Some(event) = app_gateway_event_from_notification(notification) {
                                pending_events.push(event);
                            }
                        }
                        JSONRPCMessage::Request(request) => {
                            let request_id = request.id.clone();
                            let method = request.method.clone();
                            match ServerRequest::try_from(request) {
                                Ok(request) => {
                                    pending_events.push(AppGatewayEvent::ServerRequest(request));
                                }
                                Err(err) => {
                                    warn!(%err, method, "rejecting unknown remote app-gateway request during initialize");
                                    write_jsonrpc_message(
                                        stream,
                                        JSONRPCMessage::Error(JSONRPCError {
                                            error: JSONRPCErrorError {
                                                code: -32601,
                                                message: format!(
                                                    "unsupported remote app-gateway request `{method}`"
                                                ),
                                                data: None,
                                            },
                                            id: request_id,
                                        }),
                                        websocket_url,
                                    )
                                    .await?;
                                }
                            }
                        }
                        JSONRPCMessage::Response(_) | JSONRPCMessage::Error(_) => {}
                    }
                }
                Some(Ok(Message::Binary(_)))
                | Some(Ok(Message::Ping(_)))
                | Some(Ok(Message::Pong(_)))
                | Some(Ok(Message::Frame(_))) => {}
                Some(Ok(Message::Close(frame))) => {
                    let reason = frame
                        .as_ref()
                        .map(|frame| frame.reason.to_string())
                        .filter(|reason| !reason.is_empty())
                        .unwrap_or_else(|| "connection closed during initialize".to_string());
                    break Err(IoError::new(
                        ErrorKind::ConnectionAborted,
                        format!(
                            "remote app gateway at `{websocket_url}` closed during initialize: {reason}"
                        ),
                    ));
                }
                Some(Err(err)) => {
                    break Err(IoError::other(format!(
                        "remote app gateway at `{websocket_url}` transport failed during initialize: {err}"
                    )));
                }
                None => {
                    break Err(IoError::new(
                        ErrorKind::UnexpectedEof,
                        format!("remote app gateway at `{websocket_url}` closed during initialize"),
                    ));
                }
            }
        }
    })
    .await
    .map_err(|_| {
        IoError::new(
            ErrorKind::TimedOut,
            format!("timed out waiting for initialize response from `{websocket_url}`"),
        )
    })??;

    write_jsonrpc_message(
        stream,
        JSONRPCMessage::Notification(jsonrpc_notification_from_client_notification(
            ClientNotification::Initialized,
        )),
        websocket_url,
    )
    .await?;

    Ok(pending_events)
}

fn app_gateway_event_from_notification(
    notification: JSONRPCNotification,
) -> Option<AppGatewayEvent> {
    match ServerNotification::try_from(notification) {
        Ok(notification) => Some(AppGatewayEvent::ServerNotification(notification)),
        Err(_) => None,
    }
}

async fn deliver_event(
    event_tx: &mpsc::Sender<AppGatewayEvent>,
    skipped_events: &mut usize,
    event: AppGatewayEvent,
    stream: &mut WebSocketStream<MaybeTlsStream<TcpStream>>,
) -> IoResult<()> {
    if *skipped_events > 0 {
        if event_requires_delivery(&event) {
            if event_tx
                .send(AppGatewayEvent::Lagged {
                    skipped: *skipped_events,
                })
                .await
                .is_err()
            {
                return Err(IoError::new(
                    ErrorKind::BrokenPipe,
                    "remote app-gateway event consumer channel is closed",
                ));
            }
            *skipped_events = 0;
        } else {
            match event_tx.try_send(AppGatewayEvent::Lagged {
                skipped: *skipped_events,
            }) {
                Ok(()) => *skipped_events = 0,
                Err(mpsc::error::TrySendError::Full(_)) => {
                    *skipped_events = (*skipped_events).saturating_add(1);
                    reject_if_server_request_dropped(stream, &event).await?;
                    return Ok(());
                }
                Err(mpsc::error::TrySendError::Closed(_)) => {
                    return Err(IoError::new(
                        ErrorKind::BrokenPipe,
                        "remote app-gateway event consumer channel is closed",
                    ));
                }
            }
        }
    }

    if event_requires_delivery(&event) {
        event_tx.send(event).await.map_err(|_| {
            IoError::new(
                ErrorKind::BrokenPipe,
                "remote app-gateway event consumer channel is closed",
            )
        })?;
        return Ok(());
    }

    match event_tx.try_send(event) {
        Ok(()) => Ok(()),
        Err(mpsc::error::TrySendError::Full(event)) => {
            *skipped_events = (*skipped_events).saturating_add(1);
            reject_if_server_request_dropped(stream, &event).await
        }
        Err(mpsc::error::TrySendError::Closed(_)) => Err(IoError::new(
            ErrorKind::BrokenPipe,
            "remote app-gateway event consumer channel is closed",
        )),
    }
}

async fn reject_if_server_request_dropped(
    stream: &mut WebSocketStream<MaybeTlsStream<TcpStream>>,
    event: &AppGatewayEvent,
) -> IoResult<()> {
    let AppGatewayEvent::ServerRequest(request) = event else {
        return Ok(());
    };
    write_jsonrpc_message(
        stream,
        JSONRPCMessage::Error(JSONRPCError {
            error: JSONRPCErrorError {
                code: -32001,
                message: "remote app-gateway event queue is full".to_string(),
                data: None,
            },
            id: request.id().clone(),
        }),
        "<remote-app-gateway>",
    )
    .await
}

fn event_requires_delivery(event: &AppGatewayEvent) -> bool {
    match event {
        AppGatewayEvent::ServerNotification(notification) => {
            server_notification_requires_delivery(notification)
        }
        AppGatewayEvent::Disconnected { .. } => true,
        AppGatewayEvent::Lagged { .. } | AppGatewayEvent::ServerRequest(_) => false,
    }
}

fn request_id_from_client_request(request: &ClientRequest) -> RequestId {
    jsonrpc_request_from_client_request(request.clone()).id
}

fn jsonrpc_request_from_client_request(request: ClientRequest) -> JSONRPCRequest {
    let value = match serde_json::to_value(request) {
        Ok(value) => value,
        Err(err) => panic!("client request should serialize: {err}"),
    };
    match serde_json::from_value(value) {
        Ok(request) => request,
        Err(err) => panic!("client request should encode as JSON-RPC request: {err}"),
    }
}

fn jsonrpc_notification_from_client_notification(
    notification: ClientNotification,
) -> JSONRPCNotification {
    let value = match serde_json::to_value(notification) {
        Ok(value) => value,
        Err(err) => panic!("client notification should serialize: {err}"),
    };
    match serde_json::from_value(value) {
        Ok(notification) => notification,
        Err(err) => panic!("client notification should encode as JSON-RPC notification: {err}"),
    }
}

async fn write_jsonrpc_message(
    stream: &mut WebSocketStream<MaybeTlsStream<TcpStream>>,
    message: JSONRPCMessage,
    websocket_url: &str,
) -> IoResult<()> {
    let payload = serde_json::to_string(&message).map_err(IoError::other)?;
    stream
        .send(Message::Text(payload.into()))
        .await
        .map_err(|err| {
            IoError::other(format!(
                "failed to write websocket message to `{websocket_url}`: {err}"
            ))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_requires_delivery_marks_transcript_and_disconnect_events() {
        assert!(event_requires_delivery(
            &AppGatewayEvent::ServerNotification(ServerNotification::AgentMessageDelta(
                praxis_app_gateway_protocol::AgentMessageDeltaNotification {
                    thread_id: "thread".to_string(),
                    turn_id: "turn".to_string(),
                    item_id: "item".to_string(),
                    delta: "hello".to_string(),
                },
            ),)
        ));
        assert!(event_requires_delivery(
            &AppGatewayEvent::ServerNotification(ServerNotification::ItemCompleted(
                praxis_app_gateway_protocol::ItemCompletedNotification {
                    thread_id: "thread".to_string(),
                    turn_id: "turn".to_string(),
                    item: praxis_app_gateway_protocol::ThreadItem::Plan {
                        id: "item".to_string(),
                        text: "step".to_string(),
                    },
                }
            ),)
        ));
        assert!(event_requires_delivery(&AppGatewayEvent::Disconnected {
            message: "closed".to_string(),
        }));
        assert!(!event_requires_delivery(&AppGatewayEvent::Lagged {
            skipped: 1
        }));
    }
}
