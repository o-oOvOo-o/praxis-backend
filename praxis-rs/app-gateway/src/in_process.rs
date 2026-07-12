//! In-process app-gateway runtime host for local embedders.
//!
//! This module runs the existing [`MessageProcessor`] and outbound routing logic
//! on Tokio tasks, but replaces socket/stdio transports with bounded in-memory
//! channels. The intent is to preserve app-gateway semantics while avoiding a
//! process boundary for CLI surfaces that run in the same process.
//!
//! # Lifecycle
//!
//! 1. Construct runtime state with [`InProcessStartArgs`].
//! 2. Call [`start`], which performs the `initialize` / `initialized` handshake
//!    internally and returns a ready-to-use [`InProcessClientHandle`].
//! 3. Send requests via [`InProcessClientHandle::request`], notifications via
//!    [`InProcessClientHandle::notify`], and consume events via
//!    [`InProcessClientHandle::next_event`].
//! 4. Terminate with [`InProcessClientHandle::shutdown`].
//!
//! # Transport model
//!
//! The runtime is transport-local but not protocol-free. Incoming requests are
//! typed [`ClientRequest`] values, yet responses still come back through the
//! same JSON-RPC result envelope that `MessageProcessor` uses for stdio and
//! websocket transports. This keeps in-process behavior aligned with
//! app-gateway rather than creating a second execution contract.
//!
//! # Backpressure
//!
//! Command submission uses `try_send` and can return `WouldBlock`, while event
//! fanout may drop notifications under saturation. Server requests are never
//! silently abandoned: if they cannot be queued they are failed back into
//! `MessageProcessor` with overload or internal errors so approval flows do
//! not hang indefinitely.
//!
//! # Relationship to `praxis-app-gateway-client`
//!
//! This module provides the low-level runtime handle ([`InProcessClientHandle`]).
//! Higher-level callers (TUI, exec) should go through `praxis-app-gateway-client`,
//! which wraps this module behind a worker task with async request/response
//! helpers, surface-specific startup policy, and bounded shutdown.

use std::collections::HashMap;
use std::collections::HashSet;
use std::collections::hash_map::Entry;
use std::io::Error as IoError;
use std::io::ErrorKind;
use std::io::Result as IoResult;
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::RwLock;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::time::Duration;

use crate::error_code::INTERNAL_ERROR_CODE;
use crate::error_code::INVALID_REQUEST_ERROR_CODE;
use crate::error_code::OVERLOADED_ERROR_CODE;
use crate::message_processor::ConnectionSessionState;
use crate::message_processor::MessageProcessor;
use crate::message_processor::MessageProcessorArgs;
use crate::outgoing_message::ConnectionId;
use crate::outgoing_message::OutgoingEnvelope;
use crate::outgoing_message::OutgoingMessage;
use crate::outgoing_message::OutgoingMessageSender;
use crate::outgoing_message::QueuedOutgoingMessage;
use crate::transport::AppGatewayTransport;
use crate::transport::CHANNEL_CAPACITY;
use crate::transport::ConnectionState;
use crate::transport::OutboundConnectionState;
use crate::transport::TransportEvent;
use crate::transport::auth::AppGatewayWebsocketAuthSettings;
use crate::transport::auth::policy_from_settings;
use crate::transport::route_outgoing_envelope;
use crate::transport::start_websocket_acceptor;
use praxis_analytics::AppGatewayRpcTransport;
use praxis_app_gateway_protocol::ClientNotification;
use praxis_app_gateway_protocol::ClientRequest;
use praxis_app_gateway_protocol::ConfigWarningNotification;
use praxis_app_gateway_protocol::InitializeParams;
use praxis_app_gateway_protocol::JSONRPCErrorError;
use praxis_app_gateway_protocol::JSONRPCMessage;
use praxis_app_gateway_protocol::RequestId;
use praxis_app_gateway_protocol::Result;
use praxis_app_gateway_protocol::ServerNotification;
use praxis_app_gateway_protocol::ServerRequest;
use praxis_arg0::Arg0DispatchPaths;
use praxis_core::config::Config;
use praxis_core::config_loader::CloudConfigBundleLoader;
use praxis_core::config_loader::LoaderOverrides;
use praxis_exec_server::EnvironmentManager;
use praxis_feedback::PraxisFeedback;
use praxis_protocol::protocol::SessionSource;
use tokio::sync::mpsc;
use tokio::sync::oneshot;
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;
use toml::Value as TomlValue;
use tracing::warn;

const IN_PROCESS_CONNECTION_ID: ConnectionId = ConnectionId(0);
const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(5);
/// Default bounded channel capacity for in-process runtime queues.
pub const DEFAULT_NATIVE_GATEWAY_CHANNEL_CAPACITY: usize = CHANNEL_CAPACITY;

type PendingClientRequestResponse = std::result::Result<Result, JSONRPCErrorError>;

fn server_notification_requires_delivery(notification: &ServerNotification) -> bool {
    matches!(
        notification,
        ServerNotification::TurnStarted(_)
            | ServerNotification::TurnCompleted(_)
            | ServerNotification::ItemStarted(_)
            | ServerNotification::ItemCompleted(_)
            | ServerNotification::ThreadControlChanged(_)
            | ServerNotification::ThreadGoalUpdated(_)
            | ServerNotification::ThreadGoalCleared(_)
            | ServerNotification::ThreadHeartbeatUpdated(_)
            | ServerNotification::WorkspaceChangeUpdated(_)
            | ServerNotification::AutomationRunUpdated(_)
            | ServerNotification::ThreadModelChanged(_)
            | ServerNotification::AgentMessageDelta(_)
            | ServerNotification::PlanDelta(_)
            | ServerNotification::ReasoningSummaryTextDelta(_)
            | ServerNotification::ReasoningSummaryPartAdded(_)
            | ServerNotification::ReasoningTextDelta(_)
    )
}

/// Input needed to start an in-process app-gateway runtime.
///
/// These fields mirror the pieces of ambient process state that stdio and
/// websocket transports normally assemble before `MessageProcessor` starts.
#[derive(Clone)]
pub struct InProcessStartArgs {
    /// Resolved argv0 dispatch paths used by command execution internals.
    pub arg0_paths: Arg0DispatchPaths,
    /// Shared base config used to initialize core components.
    pub config: Arc<Config>,
    /// CLI config overrides that are already parsed into TOML values.
    pub cli_overrides: Vec<(String, TomlValue)>,
    /// Loader override knobs used by config API paths.
    pub loader_overrides: LoaderOverrides,
    /// Preloaded cloud requirements provider.
    pub cloud_requirements: CloudConfigBundleLoader,
    /// Feedback sink used by app-gateway/core telemetry and logs.
    pub feedback: PraxisFeedback,
    /// Startup warnings emitted after initialize succeeds.
    pub config_warnings: Vec<ConfigWarningNotification>,
    /// Session source stamped into thread/session metadata.
    pub session_source: SessionSource,
    /// Whether auth loading should honor the legacy `CODEX_API_KEY` compatibility variable.
    pub enable_praxis_api_key_env: bool,
    /// Initialize params used for initial handshake.
    pub initialize: InitializeParams,
    /// Capacity used for all runtime queues (clamped to at least 1).
    pub channel_capacity: usize,
    /// Optional websocket listener exposing this native Center backend to external agents.
    pub control_listen: Option<SocketAddr>,
    /// Websocket auth settings for the optional external control listener.
    pub control_auth: AppGatewayWebsocketAuthSettings,
}

/// Event emitted from the app-gateway to the in-process client.
///
/// [`Lagged`](Self::Lagged) is a transport health marker, not an application
/// event — it signals that the consumer fell behind and some events were dropped.
#[derive(Debug, Clone)]
pub enum InProcessServerEvent {
    /// Server request that requires client response/rejection.
    ServerRequest(ServerRequest),
    /// App-gateway notification directed to the embedded client.
    ServerNotification(ServerNotification),
    /// Indicates one or more events were dropped due to backpressure.
    Lagged { skipped: usize },
}

/// Internal message sent from [`InProcessClientHandle`] methods to the runtime task.
///
/// Requests carry a oneshot sender for the response; notifications and server-request
/// replies are fire-and-forget from the caller's perspective (transport errors are
/// caught by `try_send` on the outer channel).
enum InProcessClientMessage {
    Request {
        request: Box<ClientRequest>,
        response_tx: oneshot::Sender<PendingClientRequestResponse>,
    },
    Notification {
        notification: ClientNotification,
    },
    ServerRequestResponse {
        request_id: RequestId,
        result: Result,
    },
    ServerRequestError {
        request_id: RequestId,
        error: JSONRPCErrorError,
    },
    Shutdown {
        done_tx: oneshot::Sender<()>,
    },
}

enum ProcessorCommand {
    Request(Box<ClientRequest>),
    Notification(ClientNotification),
}

enum InProcessOutboundControlEvent {
    Opened {
        connection_id: ConnectionId,
        writer: mpsc::Sender<QueuedOutgoingMessage>,
        disconnect_sender: Option<CancellationToken>,
        initialized: Arc<AtomicBool>,
        experimental_api_enabled: Arc<AtomicBool>,
        opted_out_notification_methods: Arc<RwLock<HashSet<String>>>,
    },
    Closed {
        connection_id: ConnectionId,
    },
    DisconnectAll,
}

#[derive(Clone)]
pub struct InProcessClientSender {
    client_tx: mpsc::Sender<InProcessClientMessage>,
}

impl InProcessClientSender {
    pub async fn request(&self, request: ClientRequest) -> IoResult<PendingClientRequestResponse> {
        let (response_tx, response_rx) = oneshot::channel();
        self.try_send_client_message(InProcessClientMessage::Request {
            request: Box::new(request),
            response_tx,
        })?;
        response_rx.await.map_err(|err| {
            IoError::new(
                ErrorKind::BrokenPipe,
                format!("in-process request response channel closed: {err}"),
            )
        })
    }

    pub fn notify(&self, notification: ClientNotification) -> IoResult<()> {
        self.try_send_client_message(InProcessClientMessage::Notification { notification })
    }

    pub fn respond_to_server_request(&self, request_id: RequestId, result: Result) -> IoResult<()> {
        self.try_send_client_message(InProcessClientMessage::ServerRequestResponse {
            request_id,
            result,
        })
    }

    pub fn fail_server_request(
        &self,
        request_id: RequestId,
        error: JSONRPCErrorError,
    ) -> IoResult<()> {
        self.try_send_client_message(InProcessClientMessage::ServerRequestError {
            request_id,
            error,
        })
    }

    fn try_send_client_message(&self, message: InProcessClientMessage) -> IoResult<()> {
        match self.client_tx.try_send(message) {
            Ok(()) => Ok(()),
            Err(mpsc::error::TrySendError::Full(_)) => Err(IoError::new(
                ErrorKind::WouldBlock,
                "in-process app-gateway client queue is full",
            )),
            Err(mpsc::error::TrySendError::Closed(_)) => Err(IoError::new(
                ErrorKind::BrokenPipe,
                "in-process app-gateway runtime is closed",
            )),
        }
    }
}

/// Handle used by an in-process client to call app-gateway and consume events.
///
/// This is the low-level runtime handle. Higher-level callers should usually go
/// through `praxis-app-gateway-client`, which adds worker-task buffering,
/// request/response helpers, and surface-specific startup policy.
pub struct InProcessClientHandle {
    client: InProcessClientSender,
    event_rx: mpsc::Receiver<InProcessServerEvent>,
    runtime_handle: tokio::task::JoinHandle<()>,
}

impl InProcessClientHandle {
    /// Sends a typed client request into the in-process runtime.
    ///
    /// The returned value is a transport-level `IoResult` containing either a
    /// JSON-RPC success payload or JSON-RPC error payload. Callers must keep
    /// request IDs unique among concurrent requests; reusing an in-flight ID
    /// produces an `INVALID_REQUEST` response and can make request routing
    /// ambiguous in the caller.
    pub async fn request(&self, request: ClientRequest) -> IoResult<PendingClientRequestResponse> {
        self.client.request(request).await
    }

    /// Sends a typed client notification into the in-process runtime.
    ///
    /// Notifications do not have an application-level response. Transport
    /// errors indicate queue saturation or closed runtime.
    pub fn notify(&self, notification: ClientNotification) -> IoResult<()> {
        self.client.notify(notification)
    }

    /// Resolves a pending [`ServerRequest`](InProcessServerEvent::ServerRequest).
    ///
    /// This should be used only with request IDs received from the current
    /// runtime event stream; sending arbitrary IDs has no effect on app-gateway
    /// state and can mask a stuck approval flow in the caller.
    pub fn respond_to_server_request(&self, request_id: RequestId, result: Result) -> IoResult<()> {
        self.client.respond_to_server_request(request_id, result)
    }

    /// Rejects a pending [`ServerRequest`](InProcessServerEvent::ServerRequest).
    ///
    /// Use this when the embedder cannot satisfy a server request; leaving
    /// requests unanswered can stall turn progress.
    pub fn fail_server_request(
        &self,
        request_id: RequestId,
        error: JSONRPCErrorError,
    ) -> IoResult<()> {
        self.client.fail_server_request(request_id, error)
    }

    /// Receives the next server event from the in-process runtime.
    ///
    /// Returns `None` when the runtime task exits and no more events are
    /// available.
    pub async fn next_event(&mut self) -> Option<InProcessServerEvent> {
        self.event_rx.recv().await
    }

    /// Requests runtime shutdown and waits for worker termination.
    ///
    /// Shutdown is bounded by internal timeouts and may abort background tasks
    /// if graceful drain does not complete in time.
    pub async fn shutdown(self) -> IoResult<()> {
        let mut runtime_handle = self.runtime_handle;
        let (done_tx, done_rx) = oneshot::channel();

        if self
            .client
            .client_tx
            .send(InProcessClientMessage::Shutdown { done_tx })
            .await
            .is_ok()
        {
            let _ = timeout(SHUTDOWN_TIMEOUT, done_rx).await;
        }

        if let Err(_elapsed) = timeout(SHUTDOWN_TIMEOUT, &mut runtime_handle).await {
            runtime_handle.abort();
            let _ = runtime_handle.await;
        }
        Ok(())
    }

    pub fn sender(&self) -> InProcessClientSender {
        self.client.clone()
    }
}

/// Starts an in-process app-gateway runtime and performs initialize handshake.
///
/// This function sends `initialize` followed by `initialized` before returning
/// the handle, so callers receive a ready-to-use runtime. If initialize fails,
/// the runtime is shut down and an `InvalidData` error is returned.
pub async fn start(args: InProcessStartArgs) -> IoResult<InProcessClientHandle> {
    let initialize = args.initialize.clone();
    let client = start_uninitialized(args).await?;

    let initialize_response = client
        .request(ClientRequest::Initialize {
            request_id: RequestId::Integer(0),
            params: initialize,
        })
        .await?;
    if let Err(error) = initialize_response {
        let _ = client.shutdown().await;
        return Err(IoError::new(
            ErrorKind::InvalidData,
            format!("in-process initialize failed: {}", error.message),
        ));
    }
    client.notify(ClientNotification::Initialized)?;

    Ok(client)
}

async fn start_uninitialized(args: InProcessStartArgs) -> IoResult<InProcessClientHandle> {
    let channel_capacity = args.channel_capacity.max(1);
    let (client_tx, mut client_rx) = mpsc::channel::<InProcessClientMessage>(channel_capacity);
    let (event_tx, event_rx) = mpsc::channel::<InProcessServerEvent>(channel_capacity);
    let (transport_event_tx, mut transport_event_rx) =
        mpsc::channel::<TransportEvent>(channel_capacity);
    let transport_shutdown_token = CancellationToken::new();
    let mut transport_accept_handles = Vec::new();
    let control_listen = args.control_listen;
    let external_transport =
        control_listen.map(|bind_address| AppGatewayTransport::WebSocket { bind_address });
    if let Some(bind_address) = control_listen {
        let accept_handle = start_websocket_acceptor(
            bind_address,
            transport_event_tx.clone(),
            transport_shutdown_token.clone(),
            policy_from_settings(&args.control_auth)?,
            /*print_startup_banner*/ false,
        )
        .await?;
        transport_accept_handles.push(accept_handle);
    }

    let runtime_handle = tokio::spawn(async move {
        let (outgoing_tx, mut outgoing_rx) = mpsc::channel::<OutgoingEnvelope>(channel_capacity);
        let outgoing_message_sender = Arc::new(OutgoingMessageSender::new(outgoing_tx));

        let (writer_tx, mut writer_rx) = mpsc::channel::<QueuedOutgoingMessage>(channel_capacity);
        let (outbound_control_tx, mut outbound_control_rx) =
            mpsc::channel::<InProcessOutboundControlEvent>(channel_capacity);
        let outbound_initialized = Arc::new(AtomicBool::new(false));
        let outbound_experimental_api_enabled = Arc::new(AtomicBool::new(false));
        let outbound_opted_out_notification_methods = Arc::new(RwLock::new(HashSet::new()));

        let mut outbound_connections = HashMap::<ConnectionId, OutboundConnectionState>::new();
        outbound_connections.insert(
            IN_PROCESS_CONNECTION_ID,
            OutboundConnectionState::new(
                writer_tx,
                Arc::clone(&outbound_initialized),
                Arc::clone(&outbound_experimental_api_enabled),
                Arc::clone(&outbound_opted_out_notification_methods),
                /*disconnect_sender*/ None,
            ),
        );
        let mut outbound_handle = tokio::spawn(async move {
            loop {
                tokio::select! {
                    biased;
                    control = outbound_control_rx.recv() => {
                        match control {
                            Some(InProcessOutboundControlEvent::Opened {
                                connection_id,
                                writer,
                                disconnect_sender,
                                initialized,
                                experimental_api_enabled,
                                opted_out_notification_methods,
                            }) => {
                                outbound_connections.insert(
                                    connection_id,
                                    OutboundConnectionState::new(
                                        writer,
                                        initialized,
                                        experimental_api_enabled,
                                        opted_out_notification_methods,
                                        disconnect_sender,
                                    ),
                                );
                            }
                            Some(InProcessOutboundControlEvent::Closed { connection_id }) => {
                                outbound_connections.remove(&connection_id);
                            }
                            Some(InProcessOutboundControlEvent::DisconnectAll) => {
                                for connection_state in outbound_connections.values() {
                                    connection_state.request_disconnect();
                                }
                                outbound_connections.retain(|connection_id, _| {
                                    *connection_id == IN_PROCESS_CONNECTION_ID
                                });
                            }
                            None => {
                                while let Some(envelope) = outgoing_rx.recv().await {
                                    route_outgoing_envelope(&mut outbound_connections, envelope)
                                        .await;
                                }
                                break;
                            }
                        }
                    }
                    envelope = outgoing_rx.recv() => {
                        let Some(envelope) = envelope else {
                            break;
                        };
                        route_outgoing_envelope(&mut outbound_connections, envelope).await;
                    }
                }
            }
            while let Some(control) = outbound_control_rx.recv().await {
                match control {
                    InProcessOutboundControlEvent::Opened {
                        connection_id,
                        writer,
                        disconnect_sender,
                        initialized,
                        experimental_api_enabled,
                        opted_out_notification_methods,
                    } => {
                        outbound_connections.insert(
                            connection_id,
                            OutboundConnectionState::new(
                                writer,
                                initialized,
                                experimental_api_enabled,
                                opted_out_notification_methods,
                                disconnect_sender,
                            ),
                        );
                    }
                    InProcessOutboundControlEvent::Closed { connection_id } => {
                        outbound_connections.remove(&connection_id);
                    }
                    InProcessOutboundControlEvent::DisconnectAll => {
                        for connection_state in outbound_connections.values() {
                            connection_state.request_disconnect();
                        }
                        outbound_connections.clear();
                    }
                }
            }
        });

        let processor_outgoing = Arc::clone(&outgoing_message_sender);
        let processor_outbound_control_tx = outbound_control_tx.clone();
        let (processor_tx, mut processor_rx) = mpsc::channel::<ProcessorCommand>(channel_capacity);
        let mut processor_handle = tokio::spawn(async move {
            let mut processor = MessageProcessor::new(MessageProcessorArgs {
                outgoing: Arc::clone(&processor_outgoing),
                arg0_paths: args.arg0_paths,
                config: args.config,
                environment_manager: Arc::new(EnvironmentManager::from_env()),
                cli_overrides: args.cli_overrides,
                loader_overrides: args.loader_overrides,
                cloud_requirements: args.cloud_requirements,
                feedback: args.feedback,
                log_db: None,
                config_warnings: args.config_warnings,
                session_source: args.session_source,
                enable_praxis_api_key_env: args.enable_praxis_api_key_env,
                rpc_transport: AppGatewayRpcTransport::InProcess,
            });
            let mut thread_created_rx = processor.thread_created_receiver();
            let mut session = ConnectionSessionState::default();
            let mut external_connections = HashMap::<ConnectionId, ConnectionState>::new();
            let mut listen_for_threads = true;

            loop {
                tokio::select! {
                    transport_event = transport_event_rx.recv(), if external_transport.is_some() => {
                        let Some(transport_event) = transport_event else {
                            break;
                        };
                        match transport_event {
                            TransportEvent::ConnectionOpened {
                                connection_id,
                                writer,
                                disconnect_sender,
                            } => {
                                let outbound_initialized = Arc::new(AtomicBool::new(false));
                                let outbound_experimental_api_enabled =
                                    Arc::new(AtomicBool::new(false));
                                let outbound_opted_out_notification_methods =
                                    Arc::new(RwLock::new(HashSet::new()));
                                if processor_outbound_control_tx
                                    .send(InProcessOutboundControlEvent::Opened {
                                        connection_id,
                                        writer,
                                        disconnect_sender,
                                        initialized: Arc::clone(&outbound_initialized),
                                        experimental_api_enabled: Arc::clone(
                                            &outbound_experimental_api_enabled,
                                        ),
                                        opted_out_notification_methods: Arc::clone(
                                            &outbound_opted_out_notification_methods,
                                        ),
                                    })
                                    .await
                                    .is_err()
                                {
                                    break;
                                }
                                external_connections.insert(
                                    connection_id,
                                    ConnectionState::new(
                                        outbound_initialized,
                                        outbound_experimental_api_enabled,
                                        outbound_opted_out_notification_methods,
                                    ),
                                );
                            }
                            TransportEvent::ConnectionClosed { connection_id } => {
                                if external_connections.remove(&connection_id).is_none() {
                                    continue;
                                }
                                if processor_outbound_control_tx
                                    .send(InProcessOutboundControlEvent::Closed { connection_id })
                                    .await
                                    .is_err()
                                {
                                    break;
                                }
                                processor.connection_closed(connection_id).await;
                            }
                            TransportEvent::IncomingMessage { connection_id, message } => {
                                match message {
                                    JSONRPCMessage::Request(request) => {
                                        let Some(connection_state) =
                                            external_connections.get_mut(&connection_id)
                                        else {
                                            warn!(
                                                "dropping request from unknown native-control connection: {connection_id:?}"
                                            );
                                            continue;
                                        };
                                        let Some(transport) = external_transport else {
                                            continue;
                                        };
                                        let was_initialized = connection_state.session.initialized;
                                        processor
                                            .process_request(
                                                connection_id,
                                                request,
                                                transport,
                                                &mut connection_state.session,
                                            )
                                            .await;
                                        if let Ok(mut opted_out_notification_methods) =
                                            connection_state
                                                .outbound_opted_out_notification_methods
                                                .write()
                                        {
                                            *opted_out_notification_methods = connection_state
                                                .session
                                                .opted_out_notification_methods
                                                .clone();
                                        } else {
                                            warn!(
                                                "failed to update outbound opted-out notifications"
                                            );
                                        }
                                        connection_state
                                            .outbound_experimental_api_enabled
                                            .store(
                                                connection_state.session.experimental_api_enabled,
                                                Ordering::Release,
                                            );
                                        if !was_initialized && connection_state.session.initialized {
                                            processor
                                                .send_initialize_notifications_to_connection(
                                                    connection_id,
                                                )
                                                .await;
                                            processor
                                                .connection_initialized(connection_id)
                                                .await;
                                            connection_state
                                                .outbound_initialized
                                                .store(true, Ordering::Release);
                                        }
                                    }
                                    JSONRPCMessage::Response(response) => {
                                        if !external_connections.contains_key(&connection_id) {
                                            warn!(
                                                "dropping response from unknown native-control connection: {connection_id:?}"
                                            );
                                            continue;
                                        }
                                        processor.process_response(connection_id, response).await;
                                    }
                                    JSONRPCMessage::Notification(notification) => {
                                        if !external_connections.contains_key(&connection_id) {
                                            warn!(
                                                "dropping notification from unknown native-control connection: {connection_id:?}"
                                            );
                                            continue;
                                        }
                                        processor.process_notification(notification).await;
                                    }
                                    JSONRPCMessage::Error(err) => {
                                        if !external_connections.contains_key(&connection_id) {
                                            warn!(
                                                "dropping error from unknown native-control connection: {connection_id:?}"
                                            );
                                            continue;
                                        }
                                        processor.process_error(connection_id, err).await;
                                    }
                                }
                            }
                        }
                    }
                    command = processor_rx.recv() => {
                        match command {
                            Some(ProcessorCommand::Request(request)) => {
                                let was_initialized = session.initialized;
                                processor
                                    .process_client_request(
                                        IN_PROCESS_CONNECTION_ID,
                                        *request,
                                        &mut session,
                                        &outbound_initialized,
                                    )
                                    .await;
                                if let Ok(mut opted_out_notification_methods) =
                                    outbound_opted_out_notification_methods.write()
                                {
                                    *opted_out_notification_methods =
                                        session.opted_out_notification_methods.clone();
                                } else {
                                    warn!("failed to update outbound opted-out notifications");
                                }
                                outbound_experimental_api_enabled.store(
                                    session.experimental_api_enabled,
                                    Ordering::Release,
                                );
                                if !was_initialized && session.initialized {
                                    processor.send_initialize_notifications().await;
                                }
                            }
                            Some(ProcessorCommand::Notification(notification)) => {
                                processor.process_client_notification(notification).await;
                            }
                            None => {
                                break;
                            }
                        }
                    }
                    created = thread_created_rx.recv(), if listen_for_threads => {
                        match created {
                            Ok(thread_id) => {
                                let mut connection_ids = Vec::<ConnectionId>::new();
                                if session.initialized {
                                    connection_ids.push(IN_PROCESS_CONNECTION_ID);
                                }
                                connection_ids.extend(external_connections.iter().filter_map(
                                    |(connection_id, connection_state)| {
                                        connection_state
                                            .session
                                            .initialized
                                            .then_some(*connection_id)
                                    },
                                ));
                                processor
                                    .try_attach_thread_listener(thread_id, connection_ids)
                                    .await;
                            }
                            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                                warn!("thread_created receiver lagged; skipping resync");
                            }
                            Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                                listen_for_threads = false;
                            }
                        }
                    }
                }
            }

            processor.clear_runtime_references();
            processor.cancel_active_login().await;
            processor.connection_closed(IN_PROCESS_CONNECTION_ID).await;
            for connection_id in external_connections.keys().copied().collect::<Vec<_>>() {
                processor.connection_closed(connection_id).await;
            }
            processor.clear_all_thread_listeners().await;
            processor.drain_background_tasks().await;
            processor.shutdown_threads().await;
        });
        let mut pending_request_responses =
            HashMap::<RequestId, oneshot::Sender<PendingClientRequestResponse>>::new();
        let mut shutdown_ack = None;

        loop {
            tokio::select! {
                message = client_rx.recv() => {
                    match message {
                        Some(InProcessClientMessage::Request { request, response_tx }) => {
                            let request = *request;
                            let request_id = request.id().clone();
                            match pending_request_responses.entry(request_id.clone()) {
                                Entry::Vacant(entry) => {
                                    entry.insert(response_tx);
                                }
                                Entry::Occupied(_) => {
                                    let _ = response_tx.send(Err(JSONRPCErrorError {
                                        code: INVALID_REQUEST_ERROR_CODE,
                                        message: format!("duplicate request id: {request_id:?}"),
                                        data: None,
                                    }));
                                    continue;
                                }
                            }

                            match processor_tx.try_send(ProcessorCommand::Request(Box::new(request))) {
                                Ok(()) => {}
                                Err(mpsc::error::TrySendError::Full(_)) => {
                                    if let Some(response_tx) =
                                        pending_request_responses.remove(&request_id)
                                    {
                                        let _ = response_tx.send(Err(JSONRPCErrorError {
                                            code: OVERLOADED_ERROR_CODE,
                                            message: "in-process app-gateway request queue is full"
                                                .to_string(),
                                            data: None,
                                        }));
                                    }
                                }
                                Err(mpsc::error::TrySendError::Closed(_)) => {
                                    if let Some(response_tx) =
                                        pending_request_responses.remove(&request_id)
                                    {
                                        let _ = response_tx.send(Err(JSONRPCErrorError {
                                            code: INTERNAL_ERROR_CODE,
                                            message:
                                                "in-process app-gateway request processor is closed"
                                                    .to_string(),
                                            data: None,
                                        }));
                                    }
                                    break;
                                }
                            }
                        }
                        Some(InProcessClientMessage::Notification { notification }) => {
                            match processor_tx.try_send(ProcessorCommand::Notification(notification)) {
                                Ok(()) => {}
                                Err(mpsc::error::TrySendError::Full(_)) => {
                                    warn!("dropping in-process client notification (queue full)");
                                }
                                Err(mpsc::error::TrySendError::Closed(_)) => {
                                    break;
                                }
                            }
                        }
                        Some(InProcessClientMessage::ServerRequestResponse { request_id, result }) => {
                            outgoing_message_sender
                                .notify_client_response(
                                    IN_PROCESS_CONNECTION_ID,
                                    request_id,
                                    result,
                                )
                                .await;
                        }
                        Some(InProcessClientMessage::ServerRequestError { request_id, error }) => {
                            outgoing_message_sender
                                .notify_client_error(
                                    IN_PROCESS_CONNECTION_ID,
                                    request_id,
                                    error,
                                )
                                .await;
                        }
                        Some(InProcessClientMessage::Shutdown { done_tx }) => {
                            shutdown_ack = Some(done_tx);
                            break;
                        }
                        None => {
                            break;
                        }
                    }
                }
                queued_message = writer_rx.recv() => {
                    let Some(queued_message) = queued_message else {
                        break;
                    };
                    let outgoing_message = queued_message.message;
                    match outgoing_message {
                        OutgoingMessage::Response(response) => {
                            if let Some(response_tx) = pending_request_responses.remove(&response.id) {
                                let _ = response_tx.send(Ok(response.result));
                            } else {
                                warn!(
                                    request_id = ?response.id,
                                    "dropping unmatched in-process response"
                                );
                            }
                        }
                        OutgoingMessage::Error(error) => {
                            if let Some(response_tx) = pending_request_responses.remove(&error.id) {
                                let _ = response_tx.send(Err(error.error));
                            } else {
                                warn!(
                                    request_id = ?error.id,
                                    "dropping unmatched in-process error response"
                                );
                            }
                        }
                        OutgoingMessage::Request(request) => {
                            // Send directly to avoid cloning; on failure the
                            // original value is returned inside the error.
                            if let Err(send_error) = event_tx
                                .try_send(InProcessServerEvent::ServerRequest(request))
                            {
                                let (code, message, inner) = match send_error {
                                    mpsc::error::TrySendError::Full(inner) => (
                                        OVERLOADED_ERROR_CODE,
                                        "in-process server request queue is full",
                                        inner,
                                    ),
                                    mpsc::error::TrySendError::Closed(inner) => (
                                        INTERNAL_ERROR_CODE,
                                        "in-process server request consumer is closed",
                                        inner,
                                    ),
                                };
                                let request_id = match inner {
                                    InProcessServerEvent::ServerRequest(req) => req.id().clone(),
                                    _ => unreachable!("we just sent a ServerRequest variant"),
                                };
                                outgoing_message_sender
                                    .notify_client_error(
                                        IN_PROCESS_CONNECTION_ID,
                                        request_id,
                                        JSONRPCErrorError {
                                            code,
                                            message: message.to_string(),
                                            data: None,
                                        },
                                    )
                                    .await;
                            }
                        }
                        OutgoingMessage::AppGatewayNotification(notification) => {
                            if server_notification_requires_delivery(&notification) {
                                if event_tx
                                    .send(InProcessServerEvent::ServerNotification(notification))
                                    .await
                                    .is_err()
                                {
                                    break;
                                }
                            } else if let Err(send_error) =
                                event_tx.try_send(InProcessServerEvent::ServerNotification(notification))
                            {
                                match send_error {
                                    mpsc::error::TrySendError::Full(_) => {
                                        warn!("dropping in-process server notification (queue full)");
                                    }
                                    mpsc::error::TrySendError::Closed(_) => {
                                        break;
                                    }
                                }
                            }
                        }
                    }
                    if let Some(write_complete_tx) = queued_message.write_complete_tx {
                        let _ = write_complete_tx.send(());
                    }
                }
            }
        }

        drop(writer_rx);
        drop(processor_tx);
        transport_shutdown_token.cancel();
        let _ = outbound_control_tx
            .send(InProcessOutboundControlEvent::DisconnectAll)
            .await;
        outgoing_message_sender
            .cancel_all_requests(Some(JSONRPCErrorError {
                code: INTERNAL_ERROR_CODE,
                message: "in-process app-gateway runtime is shutting down".to_string(),
                data: None,
            }))
            .await;
        // Drop the runtime's last sender before awaiting the router task so
        // `outgoing_rx.recv()` can observe channel closure and exit cleanly.
        drop(outgoing_message_sender);
        for (_, response_tx) in pending_request_responses {
            let _ = response_tx.send(Err(JSONRPCErrorError {
                code: INTERNAL_ERROR_CODE,
                message: "in-process app-gateway runtime is shutting down".to_string(),
                data: None,
            }));
        }

        if let Err(_elapsed) = timeout(SHUTDOWN_TIMEOUT, &mut processor_handle).await {
            processor_handle.abort();
            let _ = processor_handle.await;
        }
        if let Err(_elapsed) = timeout(SHUTDOWN_TIMEOUT, &mut outbound_handle).await {
            outbound_handle.abort();
            let _ = outbound_handle.await;
        }
        for mut handle in transport_accept_handles {
            if let Err(_elapsed) = timeout(SHUTDOWN_TIMEOUT, &mut handle).await {
                handle.abort();
                let _ = handle.await;
            }
        }

        if let Some(done_tx) = shutdown_ack {
            let _ = done_tx.send(());
        }
    });

    Ok(InProcessClientHandle {
        client: InProcessClientSender { client_tx },
        event_rx,
        runtime_handle,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use praxis_app_gateway_protocol::ClientInfo;
    use praxis_app_gateway_protocol::ConfigRequirementsReadResponse;
    use praxis_app_gateway_protocol::SessionSource as ApiSessionSource;
    use praxis_app_gateway_protocol::ThreadStartParams;
    use praxis_app_gateway_protocol::ThreadStartResponse;
    use praxis_app_gateway_protocol::Turn;
    use praxis_app_gateway_protocol::TurnCompletedNotification;
    use praxis_app_gateway_protocol::TurnStatus;
    use praxis_core::config::ConfigBuilder;
    use pretty_assertions::assert_eq;

    async fn build_test_config() -> Config {
        match ConfigBuilder::default().build().await {
            Ok(config) => config,
            Err(_) => Config::load_default_with_cli_overrides(Vec::new())
                .expect("default config should load"),
        }
    }

    async fn start_test_client_with_capacity(
        session_source: SessionSource,
        channel_capacity: usize,
    ) -> InProcessClientHandle {
        let args = InProcessStartArgs {
            arg0_paths: Arg0DispatchPaths::default(),
            config: Arc::new(build_test_config().await),
            cli_overrides: Vec::new(),
            loader_overrides: LoaderOverrides::default(),
            cloud_requirements: CloudConfigBundleLoader::default(),
            feedback: PraxisFeedback::new(),
            config_warnings: Vec::new(),
            session_source,
            enable_praxis_api_key_env: false,
            initialize: InitializeParams {
                client_info: ClientInfo {
                    name: "praxis-in-process-test".to_string(),
                    title: None,
                    version: "0.0.0".to_string(),
                },
                capabilities: None,
                host_extensions: Vec::new(),
            },
            channel_capacity,
            control_listen: None,
            control_auth: AppGatewayWebsocketAuthSettings::default(),
        };
        start(args).await.expect("in-process runtime should start")
    }

    async fn start_test_client(session_source: SessionSource) -> InProcessClientHandle {
        start_test_client_with_capacity(session_source, DEFAULT_NATIVE_GATEWAY_CHANNEL_CAPACITY)
            .await
    }

    #[tokio::test]
    async fn in_process_start_initializes_and_handles_typed_request() {
        let client = start_test_client(SessionSource::Cli).await;
        let response = client
            .request(ClientRequest::ConfigRequirementsRead {
                request_id: RequestId::Integer(1),
                params: None,
            })
            .await
            .expect("request transport should work")
            .expect("request should succeed");
        assert!(response.is_object());

        let _parsed: ConfigRequirementsReadResponse =
            serde_json::from_value(response).expect("response should match app-gateway schema");
        client
            .shutdown()
            .await
            .expect("in-process runtime should shutdown cleanly");
    }

    #[tokio::test]
    async fn in_process_start_uses_requested_session_source_for_thread_start() {
        for (requested_source, expected_source) in [
            (SessionSource::Cli, ApiSessionSource::Cli),
            (SessionSource::Exec, ApiSessionSource::Exec),
        ] {
            let client = start_test_client(requested_source).await;
            let response = client
                .request(ClientRequest::ThreadStart {
                    request_id: RequestId::Integer(2),
                    params: ThreadStartParams {
                        ephemeral: Some(true),
                        ..ThreadStartParams::default()
                    },
                })
                .await
                .expect("request transport should work")
                .expect("thread/start should succeed");
            let parsed: ThreadStartResponse =
                serde_json::from_value(response).expect("thread/start response should parse");
            assert_eq!(parsed.thread.source, expected_source);
            client
                .shutdown()
                .await
                .expect("in-process runtime should shutdown cleanly");
        }
    }

    #[tokio::test]
    async fn in_process_start_clamps_zero_channel_capacity() {
        let client =
            start_test_client_with_capacity(SessionSource::Cli, /*channel_capacity*/ 0).await;
        let response = loop {
            match client
                .request(ClientRequest::ConfigRequirementsRead {
                    request_id: RequestId::Integer(4),
                    params: None,
                })
                .await
            {
                Ok(response) => break response.expect("request should succeed"),
                Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                    tokio::task::yield_now().await;
                }
                Err(err) => panic!("request transport should work: {err}"),
            }
        };
        let _parsed: ConfigRequirementsReadResponse =
            serde_json::from_value(response).expect("response should match app-gateway schema");
        client
            .shutdown()
            .await
            .expect("in-process runtime should shutdown cleanly");
    }

    #[test]
    fn guaranteed_delivery_helpers_cover_terminal_server_notifications() {
        assert!(server_notification_requires_delivery(
            &ServerNotification::TurnCompleted(TurnCompletedNotification {
                thread_id: "thread-1".to_string(),
                turn: Turn {
                    id: "turn-1".to_string(),
                    items: Vec::new(),
                    status: TurnStatus::Completed,
                    error: None,
                },
            })
        ));
    }
}
