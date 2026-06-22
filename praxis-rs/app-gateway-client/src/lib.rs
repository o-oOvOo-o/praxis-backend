//! Shared in-process app-gateway client facade for CLI surfaces.
//!
//! This crate wraps [`praxis_app_gateway_native`] behind a single async API
//! used by surfaces like TUI and exec. It centralizes:
//!
//! - Runtime startup and initialize-capabilities handshake.
//! - Typed caller-provided startup identity (`SessionSource` + client name).
//! - Typed and raw request/notification dispatch.
//! - Server request resolution and rejection.
//! - Event consumption with backpressure signaling ([`NativeGatewayEvent::Lagged`]).
//! - Bounded graceful shutdown with abort fallback.
//!
//! The facade interposes a worker task between the caller and the underlying
//! [`NativeRuntimeHandle`](praxis_app_gateway_native::NativeRuntimeHandle),
//! bridging async `mpsc` channels on both sides. Queues are bounded so overload
//! surfaces as channel-full errors rather than unbounded memory growth.

mod remote;

use std::error::Error;
use std::fmt;
use std::future::Future;
use std::io::Error as IoError;
use std::io::ErrorKind;
use std::io::Result as IoResult;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

pub use praxis_app_gateway_native::DEFAULT_NATIVE_GATEWAY_CHANNEL_CAPACITY;
pub use praxis_app_gateway_native::NativeControlAuthSettings;
pub use praxis_app_gateway_native::NativeGatewayEvent;
use praxis_app_gateway_native::NativeRuntimeStartArgs;
use praxis_app_gateway_native::start_native_runtime;
use praxis_app_gateway_protocol::ClientInfo;
use praxis_app_gateway_protocol::ClientNotification;
use praxis_app_gateway_protocol::ClientRequest;
use praxis_app_gateway_protocol::ConfigWarningNotification;
use praxis_app_gateway_protocol::HostExtensionInfo;
use praxis_app_gateway_protocol::InitializeCapabilities;
use praxis_app_gateway_protocol::InitializeParams;
use praxis_app_gateway_protocol::JSONRPCErrorError;
use praxis_app_gateway_protocol::RequestId;
use praxis_app_gateway_protocol::Result as JsonRpcResult;
use praxis_app_gateway_protocol::ServerNotification;
use praxis_app_gateway_protocol::ServerRequest;
use praxis_arg0::Arg0DispatchPaths;
use praxis_core::config::Config;
use praxis_core::config_loader::CloudConfigBundleLoader;
use praxis_core::config_loader::LoaderOverrides;
use praxis_feedback::PraxisFeedback;
use praxis_protocol::protocol::SessionSource;
use serde::de::DeserializeOwned;
use tokio::sync::mpsc;
use tokio::sync::oneshot;
use tokio::time::timeout;
use toml::Value as TomlValue;
use tracing::warn;

pub use crate::remote::RemoteAppGatewayClient;
pub use crate::remote::RemoteAppGatewayConnectArgs;

pub const DEFAULT_LOCAL_APP_GATEWAY_URL: &str = "ws://127.0.0.1:4222";
pub const DEFAULT_LOCAL_APP_GATEWAY_ADDR: &str = "127.0.0.1:4222";

const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(5);

/// Raw app-gateway request result for typed in-process requests.
///
/// Even on the in-process path, successful responses still travel back through
/// the same JSON-RPC result envelope used by socket/stdio transports because
/// `MessageProcessor` continues to produce that shape internally.
pub type RequestResult = std::result::Result<JsonRpcResult, JSONRPCErrorError>;

#[derive(Clone, Copy)]
pub(crate) struct CommandEndpointLabels {
    pub(crate) worker_closed: &'static str,
    pub(crate) request_closed: &'static str,
    pub(crate) notify_closed: &'static str,
    pub(crate) resolve_closed: &'static str,
    pub(crate) reject_closed: &'static str,
}

pub(crate) trait AppGatewayClientCommand: Sized {
    fn request_command(
        request: Box<ClientRequest>,
        response_tx: oneshot::Sender<IoResult<RequestResult>>,
    ) -> Self;

    fn notify_command(
        notification: ClientNotification,
        response_tx: oneshot::Sender<IoResult<()>>,
    ) -> Self;

    fn resolve_server_request_command(
        request_id: RequestId,
        result: JsonRpcResult,
        response_tx: oneshot::Sender<IoResult<()>>,
    ) -> Self;

    fn reject_server_request_command(
        request_id: RequestId,
        error: JSONRPCErrorError,
        response_tx: oneshot::Sender<IoResult<()>>,
    ) -> Self;
}

pub(crate) struct AppGatewayCommandEndpoint<C: AppGatewayClientCommand> {
    command_tx: mpsc::Sender<C>,
    labels: CommandEndpointLabels,
}

impl<C: AppGatewayClientCommand> Clone for AppGatewayCommandEndpoint<C> {
    fn clone(&self) -> Self {
        Self {
            command_tx: self.command_tx.clone(),
            labels: self.labels,
        }
    }
}

impl<C: AppGatewayClientCommand> AppGatewayCommandEndpoint<C> {
    pub(crate) fn new(command_tx: mpsc::Sender<C>, labels: CommandEndpointLabels) -> Self {
        Self { command_tx, labels }
    }

    pub(crate) async fn request(&self, request: ClientRequest) -> IoResult<RequestResult> {
        send_request_command(
            &self.command_tx,
            request,
            C::request_command,
            self.labels.worker_closed,
            self.labels.request_closed,
        )
        .await
    }

    pub(crate) async fn request_typed<T>(
        &self,
        request: ClientRequest,
    ) -> Result<T, TypedRequestError>
    where
        T: DeserializeOwned,
    {
        request_typed_with(request, |request| self.request(request)).await
    }

    pub(crate) async fn notify(&self, notification: ClientNotification) -> IoResult<()> {
        send_unit_command(
            &self.command_tx,
            |response_tx| C::notify_command(notification, response_tx),
            self.labels.worker_closed,
            self.labels.notify_closed,
        )
        .await
    }

    pub(crate) async fn resolve_server_request(
        &self,
        request_id: RequestId,
        result: JsonRpcResult,
    ) -> IoResult<()> {
        send_unit_command(
            &self.command_tx,
            |response_tx| C::resolve_server_request_command(request_id, result, response_tx),
            self.labels.worker_closed,
            self.labels.resolve_closed,
        )
        .await
    }

    pub(crate) async fn reject_server_request(
        &self,
        request_id: RequestId,
        error: JSONRPCErrorError,
    ) -> IoResult<()> {
        send_unit_command(
            &self.command_tx,
            |response_tx| C::reject_server_request_command(request_id, error, response_tx),
            self.labels.worker_closed,
            self.labels.reject_closed,
        )
        .await
    }
}

fn initialize_params_from_metadata(
    client_name: &str,
    client_version: &str,
    experimental_api: bool,
    opt_out_notification_methods: &[String],
    host_extensions: Vec<HostExtensionInfo>,
) -> InitializeParams {
    let capabilities = InitializeCapabilities {
        experimental_api,
        opt_out_notification_methods: if opt_out_notification_methods.is_empty() {
            None
        } else {
            Some(opt_out_notification_methods.to_vec())
        },
    };

    InitializeParams {
        client_info: ClientInfo {
            name: client_name.to_string(),
            title: None,
            version: client_version.to_string(),
        },
        capabilities: Some(capabilities),
        host_extensions,
    }
}

async fn send_request_command<C>(
    command_tx: &mpsc::Sender<C>,
    request: ClientRequest,
    make_command: impl FnOnce(Box<ClientRequest>, oneshot::Sender<IoResult<RequestResult>>) -> C,
    worker_closed_message: &'static str,
    response_closed_message: &'static str,
) -> IoResult<RequestResult> {
    let (response_tx, response_rx) = oneshot::channel();
    command_tx
        .send(make_command(Box::new(request), response_tx))
        .await
        .map_err(|_| IoError::new(ErrorKind::BrokenPipe, worker_closed_message))?;
    response_rx
        .await
        .map_err(|_| IoError::new(ErrorKind::BrokenPipe, response_closed_message))?
}

async fn send_unit_command<C>(
    command_tx: &mpsc::Sender<C>,
    make_command: impl FnOnce(oneshot::Sender<IoResult<()>>) -> C,
    worker_closed_message: &'static str,
    response_closed_message: &'static str,
) -> IoResult<()> {
    let (response_tx, response_rx) = oneshot::channel();
    command_tx
        .send(make_command(response_tx))
        .await
        .map_err(|_| IoError::new(ErrorKind::BrokenPipe, worker_closed_message))?;
    response_rx
        .await
        .map_err(|_| IoError::new(ErrorKind::BrokenPipe, response_closed_message))?
}

async fn request_typed_with<T, F, Fut>(
    request: ClientRequest,
    send_request: F,
) -> Result<T, TypedRequestError>
where
    T: DeserializeOwned,
    F: FnOnce(ClientRequest) -> Fut,
    Fut: Future<Output = IoResult<RequestResult>>,
{
    let method = request_method_name(&request);
    let response = send_request(request)
        .await
        .map_err(|source| TypedRequestError::Transport {
            method: method.clone(),
            source,
        })?;
    let result = response.map_err(|source| TypedRequestError::Server {
        method: method.clone(),
        source,
    })?;
    serde_json::from_value(result)
        .map_err(|source| TypedRequestError::Deserialize { method, source })
}

#[derive(Debug, Clone)]
pub enum AppGatewayEvent {
    Lagged { skipped: usize },
    ServerNotification(ServerNotification),
    ServerRequest(ServerRequest),
    Disconnected { message: String },
}

impl From<NativeGatewayEvent> for AppGatewayEvent {
    fn from(value: NativeGatewayEvent) -> Self {
        match value {
            NativeGatewayEvent::Lagged { skipped } => Self::Lagged { skipped },
            NativeGatewayEvent::Notification(notification) => {
                Self::ServerNotification(notification)
            }
            NativeGatewayEvent::ServerRequest(request) => Self::ServerRequest(request),
        }
    }
}

fn event_requires_delivery(event: &NativeGatewayEvent) -> bool {
    // These transcript and terminal events must remain lossless. Dropping
    // streamed assistant text or the authoritative completed item can leave
    // the TUI with permanently corrupted markdown, while dropping completion
    // notifications can leave surfaces waiting forever.
    match event {
        NativeGatewayEvent::Notification(notification) => {
            server_notification_requires_delivery(notification)
        }
        _ => false,
    }
}

/// Returns `true` for notifications that must survive backpressure.
///
/// Turn boundaries, transcript events (`AgentMessageDelta`, `PlanDelta`,
/// reasoning deltas), and authoritative item completions form the lossless tier
/// of the event stream. Dropping any of these corrupts the visible assistant
/// output or leaves surfaces waiting for a completion signal that already
/// fired. Everything else (`CommandExecutionOutputDelta`, progress, etc.) is
/// best-effort and may be dropped with only cosmetic impact.
///
/// Both the in-process and remote transports delegate to this function so the
/// classification stays in sync.
pub(crate) fn server_notification_requires_delivery(notification: &ServerNotification) -> bool {
    matches!(
        notification,
        ServerNotification::TurnStarted(_)
            | ServerNotification::TurnCompleted(_)
            | ServerNotification::ItemStarted(_)
            | ServerNotification::ItemCompleted(_)
            | ServerNotification::ThreadGoalUpdated(_)
            | ServerNotification::ThreadGoalCleared(_)
            | ServerNotification::ThreadModelChanged(_)
            | ServerNotification::AgentMessageDelta(_)
            | ServerNotification::PlanDelta(_)
            | ServerNotification::ReasoningSummaryTextDelta(_)
            | ServerNotification::ReasoningSummaryPartAdded(_)
            | ServerNotification::ReasoningTextDelta(_)
    )
}

/// Outcome of attempting to forward a single event to the consumer channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ForwardEventResult {
    /// The event was delivered (or intentionally dropped); the stream is healthy.
    Continue,
    /// The consumer channel is closed; the caller should stop producing events.
    DisableStream,
}

/// Forwards a single in-process event to the consumer, respecting the
/// lossless/best-effort split.
///
/// Lossless events (transcript deltas, item/turn completions) block until the
/// consumer drains capacity. Best-effort events use `try_send` and increment
/// `skipped_events` on failure. When a lag marker needs to be flushed before a
/// lossless event, the flush itself blocks so the marker is never lost.
///
/// If a dropped event is a `ServerRequest`, `reject_server_request` is called
/// so the server does not wait for a response that will never come.
async fn forward_in_process_event<F>(
    event_tx: &mpsc::Sender<NativeGatewayEvent>,
    skipped_events: &mut usize,
    event: NativeGatewayEvent,
    mut reject_server_request: F,
) -> ForwardEventResult
where
    F: FnMut(ServerRequest),
{
    if *skipped_events > 0 {
        if event_requires_delivery(&event) {
            // Surface lag before the lossless event, but do not let the lag marker itself cause
            // us to drop the transcript/completion notification the caller is blocked on.
            if event_tx
                .send(NativeGatewayEvent::Lagged {
                    skipped: *skipped_events,
                })
                .await
                .is_err()
            {
                return ForwardEventResult::DisableStream;
            }
            *skipped_events = 0;
        } else {
            match event_tx.try_send(NativeGatewayEvent::Lagged {
                skipped: *skipped_events,
            }) {
                Ok(()) => {
                    *skipped_events = 0;
                }
                Err(mpsc::error::TrySendError::Full(_)) => {
                    *skipped_events = skipped_events.saturating_add(1);
                    warn!("dropping in-process app-gateway event because consumer queue is full");
                    if let NativeGatewayEvent::ServerRequest(request) = event {
                        reject_server_request(request);
                    }
                    return ForwardEventResult::Continue;
                }
                Err(mpsc::error::TrySendError::Closed(_)) => {
                    return ForwardEventResult::DisableStream;
                }
            }
        }
    }

    if event_requires_delivery(&event) {
        // Block until the consumer catches up for transcript/completion notifications; this
        // preserves the visible assistant output even when the queue is otherwise saturated.
        if event_tx.send(event).await.is_err() {
            return ForwardEventResult::DisableStream;
        }
        return ForwardEventResult::Continue;
    }

    match event_tx.try_send(event) {
        Ok(()) => ForwardEventResult::Continue,
        Err(mpsc::error::TrySendError::Full(event)) => {
            *skipped_events = skipped_events.saturating_add(1);
            warn!("dropping in-process app-gateway event because consumer queue is full");
            if let NativeGatewayEvent::ServerRequest(request) = event {
                reject_server_request(request);
            }
            ForwardEventResult::Continue
        }
        Err(mpsc::error::TrySendError::Closed(_)) => ForwardEventResult::DisableStream,
    }
}

/// Layered error for [`NativeAppGatewayClient::request_typed`].
///
/// This keeps transport failures, server-side JSON-RPC failures, and response
/// decode failures distinct so callers can decide whether to retry, surface a
/// server error, or treat the response as an internal request/response mismatch.
#[derive(Debug)]
pub enum TypedRequestError {
    Transport {
        method: String,
        source: IoError,
    },
    Server {
        method: String,
        source: JSONRPCErrorError,
    },
    Deserialize {
        method: String,
        source: serde_json::Error,
    },
}

impl fmt::Display for TypedRequestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Transport { method, source } => {
                write!(f, "{method} transport error: {source}")
            }
            Self::Server { method, source } => {
                write!(f, "{method} failed: {}", source.message)
            }
            Self::Deserialize { method, source } => {
                write!(f, "{method} response decode error: {source}")
            }
        }
    }
}

impl Error for TypedRequestError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Transport { source, .. } => Some(source),
            Self::Server { .. } => None,
            Self::Deserialize { source, .. } => Some(source),
        }
    }
}

#[derive(Clone)]
pub struct NativeAppGatewayClientStartArgs {
    /// Resolved argv0 dispatch paths used by command execution internals.
    pub arg0_paths: Arg0DispatchPaths,
    /// Shared config used to initialize app-gateway runtime.
    pub config: Arc<Config>,
    /// CLI config overrides that are already parsed into TOML values.
    pub cli_overrides: Vec<(String, TomlValue)>,
    /// Loader override knobs used by config API paths.
    pub loader_overrides: LoaderOverrides,
    /// Preloaded cloud config bundle provider.
    pub cloud_requirements: CloudConfigBundleLoader,
    /// Feedback sink used by app-gateway/core telemetry and logs.
    pub feedback: PraxisFeedback,
    /// Startup warnings emitted after initialize succeeds.
    pub config_warnings: Vec<ConfigWarningNotification>,
    /// Session source recorded in app-gateway thread metadata.
    pub session_source: SessionSource,
    /// Whether auth loading should honor the `CODEX_API_KEY` environment variable.
    pub enable_praxis_api_key_env: bool,
    /// Client name reported during initialize.
    pub client_name: String,
    /// Client version reported during initialize.
    pub client_version: String,
    /// Whether experimental APIs are requested at initialize time.
    pub experimental_api: bool,
    /// Notification methods this client opts out of receiving.
    pub opt_out_notification_methods: Vec<String>,
    /// Host extensions exposed by this app-gateway client.
    pub host_extensions: Vec<HostExtensionInfo>,
    /// Queue capacity for command/event channels (clamped to at least 1).
    pub channel_capacity: usize,
    /// Optional websocket listener exposing this native backend to external agents.
    pub control_listen: Option<SocketAddr>,
    /// Auth settings for the optional native external-control listener.
    pub control_auth: NativeControlAuthSettings,
}

impl NativeAppGatewayClientStartArgs {
    /// Builds initialize params from caller-provided metadata.
    pub fn initialize_params(&self) -> InitializeParams {
        initialize_params_from_metadata(
            self.client_name.as_str(),
            self.client_version.as_str(),
            self.experimental_api,
            &self.opt_out_notification_methods,
            self.host_extensions.clone(),
        )
    }

    fn into_runtime_start_args(self) -> NativeRuntimeStartArgs {
        let initialize = self.initialize_params();
        NativeRuntimeStartArgs {
            arg0_paths: self.arg0_paths,
            config: self.config,
            cli_overrides: self.cli_overrides,
            loader_overrides: self.loader_overrides,
            cloud_requirements: self.cloud_requirements,
            feedback: self.feedback,
            config_warnings: self.config_warnings,
            session_source: self.session_source,
            enable_praxis_api_key_env: self.enable_praxis_api_key_env,
            initialize,
            channel_capacity: self.channel_capacity,
            control_listen: self.control_listen,
            control_auth: self.control_auth,
        }
    }
}

/// Internal command sent from public facade methods to the worker task.
///
/// Each variant carries a oneshot sender so the caller can `await` the
/// result without holding a mutable reference to the client.
enum ClientCommand {
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

impl AppGatewayClientCommand for ClientCommand {
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

fn native_command_endpoint(
    command_tx: mpsc::Sender<ClientCommand>,
) -> AppGatewayCommandEndpoint<ClientCommand> {
    AppGatewayCommandEndpoint::new(
        command_tx,
        CommandEndpointLabels {
            worker_closed: "in-process app-gateway worker channel is closed",
            request_closed: "in-process app-gateway request channel is closed",
            notify_closed: "in-process app-gateway notify channel is closed",
            resolve_closed: "in-process app-gateway resolve channel is closed",
            reject_closed: "in-process app-gateway reject channel is closed",
        },
    )
}

/// Async facade over the in-process app-gateway runtime.
///
/// This type owns a worker task that bridges between:
/// - caller-facing async `mpsc` channels used by TUI/exec
/// - [`praxis_app_gateway_native::NativeRuntimeHandle`], which speaks to
///   the embedded `MessageProcessor`
///
/// The facade intentionally preserves the server's request/notification/event
/// model instead of exposing direct core runtime handles. That keeps in-process
/// callers aligned with app-gateway behavior while still avoiding a process
/// boundary.
pub struct NativeAppGatewayClient {
    command_tx: mpsc::Sender<ClientCommand>,
    command_endpoint: AppGatewayCommandEndpoint<ClientCommand>,
    event_rx: mpsc::Receiver<NativeGatewayEvent>,
    worker_handle: tokio::task::JoinHandle<()>,
}

#[derive(Clone)]
pub struct NativeAppGatewayRequestHandle {
    command_endpoint: AppGatewayCommandEndpoint<ClientCommand>,
}

#[derive(Clone)]
pub enum AppGatewayRequestHandle {
    Native(NativeAppGatewayRequestHandle),
    Remote(crate::remote::RemoteAppGatewayRequestHandle),
}

pub enum AppGatewayClient {
    Native(NativeAppGatewayClient),
    Remote(RemoteAppGatewayClient),
}

impl NativeAppGatewayClient {
    /// Starts the in-process runtime and facade worker task.
    ///
    /// The returned client is ready for requests and event consumption. If the
    /// internal event queue is saturated later, server requests are rejected
    /// with overload error instead of being silently dropped.
    pub async fn start(args: NativeAppGatewayClientStartArgs) -> IoResult<Self> {
        let channel_capacity = args.channel_capacity.max(1);
        let mut handle = start_native_runtime(args.into_runtime_start_args()).await?;
        let request_sender = handle.sender();
        let (command_tx, mut command_rx) = mpsc::channel::<ClientCommand>(channel_capacity);
        let (event_tx, event_rx) = mpsc::channel::<NativeGatewayEvent>(channel_capacity);

        let worker_handle = tokio::spawn(async move {
            let mut event_stream_enabled = true;
            let mut skipped_events = 0usize;
            loop {
                tokio::select! {
                    command = command_rx.recv() => {
                        match command {
                            Some(ClientCommand::Request { request, response_tx }) => {
                                let request_sender = request_sender.clone();
                                // Request waits happen on a detached task so
                                // this loop can keep draining runtime events
                                // while the request is blocked on client input.
                                tokio::spawn(async move {
                                    let result = request_sender.request(*request).await;
                                    let _ = response_tx.send(result);
                                });
                            }
                            Some(ClientCommand::Notify {
                                notification,
                                response_tx,
                            }) => {
                                let result = request_sender.notify(notification);
                                let _ = response_tx.send(result);
                            }
                            Some(ClientCommand::ResolveServerRequest {
                                request_id,
                                result,
                                response_tx,
                            }) => {
                                let send_result =
                                    request_sender.respond_to_server_request(request_id, result);
                                let _ = response_tx.send(send_result);
                            }
                            Some(ClientCommand::RejectServerRequest {
                                request_id,
                                error,
                                response_tx,
                            }) => {
                                let send_result = request_sender.fail_server_request(request_id, error);
                                let _ = response_tx.send(send_result);
                            }
                            Some(ClientCommand::Shutdown { response_tx }) => {
                                let shutdown_result = handle.shutdown().await;
                                let _ = response_tx.send(shutdown_result);
                                break;
                            }
                            None => {
                                let _ = handle.shutdown().await;
                                break;
                            }
                        }
                    }
                    event = handle.next_event(), if event_stream_enabled => {
                        let Some(event) = event else {
                            break;
                        };
                        if let NativeGatewayEvent::ServerRequest(
                            ServerRequest::ChatgptAuthTokensRefresh { request_id, .. }
                        ) = &event
                        {
                            let send_result = request_sender.fail_server_request(
                                request_id.clone(),
                                JSONRPCErrorError {
                                    code: -32000,
                                    message: "chatgpt auth token refresh is not supported for in-process app-gateway clients".to_string(),
                                    data: None,
                                },
                            );
                            if let Err(err) = send_result {
                                warn!(
                                    "failed to reject unsupported chatgpt auth token refresh request: {err}"
                                );
                            }
                            continue;
                        }

                        match forward_in_process_event(
                            &event_tx,
                            &mut skipped_events,
                            event,
                            |request| {
                                let _ = request_sender.fail_server_request(
                                    request.id().clone(),
                                    JSONRPCErrorError {
                                        code: -32001,
                                        message: "in-process app-gateway event queue is full"
                                            .to_string(),
                                        data: None,
                                    },
                                );
                            },
                        )
                        .await
                        {
                            ForwardEventResult::Continue => {}
                            ForwardEventResult::DisableStream => {
                                event_stream_enabled = false;
                            }
                        }
                    }
                }
            }
        });

        let command_endpoint = native_command_endpoint(command_tx.clone());
        Ok(Self {
            command_tx,
            command_endpoint,
            event_rx,
            worker_handle,
        })
    }

    pub fn request_handle(&self) -> NativeAppGatewayRequestHandle {
        NativeAppGatewayRequestHandle {
            command_endpoint: self.command_endpoint.clone(),
        }
    }

    /// Sends a typed client request and returns raw JSON-RPC result.
    ///
    /// Callers that expect a concrete response type should usually prefer
    /// [`request_typed`](Self::request_typed).
    pub async fn request(&self, request: ClientRequest) -> IoResult<RequestResult> {
        self.command_endpoint.request(request).await
    }

    /// Sends a typed client request and decodes the successful response body.
    ///
    /// This still deserializes from a JSON value produced by app-gateway's
    /// JSON-RPC result envelope. Because the caller chooses `T`, `Deserialize`
    /// failures indicate an internal request/response mismatch at the call site
    /// (or an in-process bug), not transport skew from an external client.
    pub async fn request_typed<T>(&self, request: ClientRequest) -> Result<T, TypedRequestError>
    where
        T: DeserializeOwned,
    {
        self.command_endpoint.request_typed(request).await
    }

    /// Sends a typed client notification.
    pub async fn notify(&self, notification: ClientNotification) -> IoResult<()> {
        self.command_endpoint.notify(notification).await
    }

    /// Resolves a pending server request.
    ///
    /// This should only be called with request IDs obtained from the current
    /// client's event stream.
    pub async fn resolve_server_request(
        &self,
        request_id: RequestId,
        result: JsonRpcResult,
    ) -> IoResult<()> {
        self.command_endpoint
            .resolve_server_request(request_id, result)
            .await
    }

    /// Rejects a pending server request with JSON-RPC error payload.
    pub async fn reject_server_request(
        &self,
        request_id: RequestId,
        error: JSONRPCErrorError,
    ) -> IoResult<()> {
        self.command_endpoint
            .reject_server_request(request_id, error)
            .await
    }

    /// Returns the next in-process event, or `None` when worker exits.
    ///
    /// Callers are expected to drain this stream promptly. If they fall behind,
    /// the worker emits [`NativeGatewayEvent::Lagged`] markers and may reject
    /// pending server requests rather than letting approval flows hang.
    pub async fn next_event(&mut self) -> Option<NativeGatewayEvent> {
        self.event_rx.recv().await
    }

    pub fn try_next_event(&mut self) -> Option<NativeGatewayEvent> {
        self.event_rx.try_recv().ok()
    }

    /// Shuts down worker and in-process runtime with bounded wait.
    ///
    /// If graceful shutdown exceeds timeout, the worker task is aborted to
    /// avoid leaking background tasks in embedding callers.
    pub async fn shutdown(self) -> IoResult<()> {
        let Self {
            command_tx,
            command_endpoint: _command_endpoint,
            event_rx,
            worker_handle,
        } = self;
        let mut worker_handle = worker_handle;
        // Drop the caller-facing receiver before asking the worker to shut
        // down. That unblocks any pending must-deliver `event_tx.send(..)`
        // so the worker can reach `handle.shutdown()` instead of timing out
        // and getting aborted with the runtime still attached.
        drop(event_rx);
        let (response_tx, response_rx) = oneshot::channel();
        if command_tx
            .send(ClientCommand::Shutdown { response_tx })
            .await
            .is_ok()
            && let Ok(command_result) = timeout(SHUTDOWN_TIMEOUT, response_rx).await
        {
            command_result.map_err(|_| {
                IoError::new(
                    ErrorKind::BrokenPipe,
                    "in-process app-gateway shutdown channel is closed",
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

impl NativeAppGatewayRequestHandle {
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

impl AppGatewayRequestHandle {
    pub async fn request(&self, request: ClientRequest) -> IoResult<RequestResult> {
        match self {
            Self::Native(handle) => handle.request(request).await,
            Self::Remote(handle) => handle.request(request).await,
        }
    }

    pub async fn request_typed<T>(&self, request: ClientRequest) -> Result<T, TypedRequestError>
    where
        T: DeserializeOwned,
    {
        match self {
            Self::Native(handle) => handle.request_typed(request).await,
            Self::Remote(handle) => handle.request_typed(request).await,
        }
    }
}

impl AppGatewayClient {
    pub async fn request(&self, request: ClientRequest) -> IoResult<RequestResult> {
        match self {
            Self::Native(client) => client.request(request).await,
            Self::Remote(client) => client.request(request).await,
        }
    }

    pub async fn request_typed<T>(&self, request: ClientRequest) -> Result<T, TypedRequestError>
    where
        T: DeserializeOwned,
    {
        match self {
            Self::Native(client) => client.request_typed(request).await,
            Self::Remote(client) => client.request_typed(request).await,
        }
    }

    pub async fn notify(&self, notification: ClientNotification) -> IoResult<()> {
        match self {
            Self::Native(client) => client.notify(notification).await,
            Self::Remote(client) => client.notify(notification).await,
        }
    }

    pub async fn resolve_server_request(
        &self,
        request_id: RequestId,
        result: JsonRpcResult,
    ) -> IoResult<()> {
        match self {
            Self::Native(client) => client.resolve_server_request(request_id, result).await,
            Self::Remote(client) => client.resolve_server_request(request_id, result).await,
        }
    }

    pub async fn reject_server_request(
        &self,
        request_id: RequestId,
        error: JSONRPCErrorError,
    ) -> IoResult<()> {
        match self {
            Self::Native(client) => client.reject_server_request(request_id, error).await,
            Self::Remote(client) => client.reject_server_request(request_id, error).await,
        }
    }

    pub async fn next_event(&mut self) -> Option<AppGatewayEvent> {
        match self {
            Self::Native(client) => client.next_event().await.map(Into::into),
            Self::Remote(client) => client.next_event().await,
        }
    }

    pub fn try_next_event(&mut self) -> Option<AppGatewayEvent> {
        match self {
            Self::Native(client) => client.try_next_event().map(Into::into),
            Self::Remote(client) => client.try_next_event(),
        }
    }

    pub async fn shutdown(self) -> IoResult<()> {
        match self {
            Self::Native(client) => client.shutdown().await,
            Self::Remote(client) => client.shutdown().await,
        }
    }

    pub fn request_handle(&self) -> AppGatewayRequestHandle {
        match self {
            Self::Native(client) => AppGatewayRequestHandle::Native(client.request_handle()),
            Self::Remote(client) => AppGatewayRequestHandle::Remote(client.request_handle()),
        }
    }
}

/// Extracts the JSON-RPC method name for diagnostics without extending the
/// protocol crate with in-process-only helpers.
pub(crate) fn request_method_name(request: &ClientRequest) -> String {
    serde_json::to_value(request)
        .ok()
        .and_then(|value| {
            value
                .get("method")
                .and_then(serde_json::Value::as_str)
                .map(ToOwned::to_owned)
        })
        .unwrap_or_else(|| "<unknown>".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::SinkExt;
    use futures::StreamExt;
    use praxis_app_gateway_protocol::AccountUpdatedNotification;
    use praxis_app_gateway_protocol::ConfigRequirementsReadResponse;
    use praxis_app_gateway_protocol::GetAccountResponse;
    use praxis_app_gateway_protocol::JSONRPCMessage;
    use praxis_app_gateway_protocol::JSONRPCRequest;
    use praxis_app_gateway_protocol::JSONRPCResponse;
    use praxis_app_gateway_protocol::ServerNotification;
    use praxis_app_gateway_protocol::SessionSource as ApiSessionSource;
    use praxis_app_gateway_protocol::ThreadStartParams;
    use praxis_app_gateway_protocol::ThreadStartResponse;
    use praxis_app_gateway_protocol::ToolRequestUserInputParams;
    use praxis_app_gateway_protocol::ToolRequestUserInputQuestion;
    use praxis_core::config::ConfigBuilder;
    use pretty_assertions::assert_eq;
    use tokio::net::TcpListener;
    use tokio::time::Duration;
    use tokio::time::timeout;
    use tokio_tungstenite::accept_hdr_async;
    use tokio_tungstenite::tungstenite::Message;
    use tokio_tungstenite::tungstenite::handshake::server::Request as WebSocketRequest;
    use tokio_tungstenite::tungstenite::handshake::server::Response as WebSocketResponse;
    use tokio_tungstenite::tungstenite::http::header::AUTHORIZATION;

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
    ) -> NativeAppGatewayClient {
        NativeAppGatewayClient::start(NativeAppGatewayClientStartArgs {
            arg0_paths: Arg0DispatchPaths::default(),
            config: Arc::new(build_test_config().await),
            cli_overrides: Vec::new(),
            loader_overrides: LoaderOverrides::default(),
            cloud_requirements: CloudConfigBundleLoader::default(),
            feedback: PraxisFeedback::new(),
            config_warnings: Vec::new(),
            session_source,
            enable_praxis_api_key_env: false,
            client_name: "praxis-app-gateway-client-test".to_string(),
            client_version: "0.0.0-test".to_string(),
            experimental_api: true,
            opt_out_notification_methods: Vec::new(),
            host_extensions: Vec::new(),
            channel_capacity,
            control_listen: None,
            control_auth: NativeControlAuthSettings::default(),
        })
        .await
        .expect("in-process app-gateway client should start")
    }

    async fn start_test_client(session_source: SessionSource) -> NativeAppGatewayClient {
        start_test_client_with_capacity(session_source, DEFAULT_NATIVE_GATEWAY_CHANNEL_CAPACITY)
            .await
    }

    async fn start_test_remote_server<F, Fut>(handler: F) -> String
    where
        F: FnOnce(tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>) -> Fut
            + Send
            + 'static,
        Fut: std::future::Future<Output = ()> + Send + 'static,
    {
        start_test_remote_server_with_auth(/*expected_auth_token*/ None, handler).await
    }

    async fn start_test_remote_server_with_auth<F, Fut>(
        expected_auth_token: Option<String>,
        handler: F,
    ) -> String
    where
        F: FnOnce(tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>) -> Fut
            + Send
            + 'static,
        Fut: std::future::Future<Output = ()> + Send + 'static,
    {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener should bind");
        let addr = listener.local_addr().expect("listener address");
        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.expect("accept should succeed");
            let websocket = accept_hdr_async(
                stream,
                move |request: &WebSocketRequest, response: WebSocketResponse| {
                    let provided_auth_token = request
                        .headers()
                        .get(AUTHORIZATION)
                        .and_then(|value| value.to_str().ok())
                        .map(str::to_owned);
                    let expected_auth_token = expected_auth_token
                        .as_ref()
                        .map(|token| format!("Bearer {token}"));
                    assert_eq!(provided_auth_token, expected_auth_token);
                    Ok(response)
                },
            )
            .await
            .expect("websocket upgrade should succeed");
            handler(websocket).await;
        });
        format!("ws://{addr}")
    }

    async fn expect_remote_initialize(
        websocket: &mut tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>,
    ) {
        let JSONRPCMessage::Request(request) = read_websocket_message(websocket).await else {
            panic!("expected initialize request");
        };
        assert_eq!(request.method, "initialize");
        write_websocket_message(
            websocket,
            JSONRPCMessage::Response(JSONRPCResponse {
                id: request.id,
                result: serde_json::json!({}),
            }),
        )
        .await;

        let JSONRPCMessage::Notification(notification) = read_websocket_message(websocket).await
        else {
            panic!("expected initialized notification");
        };
        assert_eq!(notification.method, "initialized");
    }

    async fn read_websocket_message(
        websocket: &mut tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>,
    ) -> JSONRPCMessage {
        loop {
            let frame = websocket
                .next()
                .await
                .expect("frame should be available")
                .expect("frame should decode");
            match frame {
                Message::Text(text) => {
                    return serde_json::from_str::<JSONRPCMessage>(&text)
                        .expect("text frame should be valid JSON-RPC");
                }
                Message::Binary(_) | Message::Ping(_) | Message::Pong(_) | Message::Frame(_) => {
                    continue;
                }
                Message::Close(_) => panic!("unexpected close frame"),
            }
        }
    }

    async fn write_websocket_message(
        websocket: &mut tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>,
        message: JSONRPCMessage,
    ) {
        websocket
            .send(Message::Text(
                serde_json::to_string(&message)
                    .expect("message should serialize")
                    .into(),
            ))
            .await
            .expect("message should send");
    }

    fn command_execution_output_delta_notification(delta: &str) -> ServerNotification {
        ServerNotification::CommandExecutionOutputDelta(
            praxis_app_gateway_protocol::CommandExecutionOutputDeltaNotification {
                thread_id: "thread".to_string(),
                turn_id: "turn".to_string(),
                item_id: "item".to_string(),
                delta: delta.to_string(),
            },
        )
    }

    fn agent_message_delta_notification(delta: &str) -> ServerNotification {
        ServerNotification::AgentMessageDelta(
            praxis_app_gateway_protocol::AgentMessageDeltaNotification {
                thread_id: "thread".to_string(),
                turn_id: "turn".to_string(),
                item_id: "item".to_string(),
                delta: delta.to_string(),
            },
        )
    }

    fn item_completed_notification(text: &str) -> ServerNotification {
        ServerNotification::ItemCompleted(praxis_app_gateway_protocol::ItemCompletedNotification {
            thread_id: "thread".to_string(),
            turn_id: "turn".to_string(),
            item: praxis_app_gateway_protocol::ThreadItem::AgentMessage {
                id: "item".to_string(),
                text: text.to_string(),
                phase: None,
                memory_citation: None,
            },
        })
    }

    fn turn_completed_notification() -> ServerNotification {
        ServerNotification::TurnCompleted(praxis_app_gateway_protocol::TurnCompletedNotification {
            thread_id: "thread".to_string(),
            turn: praxis_app_gateway_protocol::Turn {
                id: "turn".to_string(),
                items: Vec::new(),
                status: praxis_app_gateway_protocol::TurnStatus::Completed,
                error: None,
            },
        })
    }

    fn test_remote_connect_args(websocket_url: String) -> RemoteAppGatewayConnectArgs {
        RemoteAppGatewayConnectArgs {
            websocket_url,
            auth_token: None,
            client_name: "praxis-app-gateway-client-test".to_string(),
            client_version: "0.0.0-test".to_string(),
            experimental_api: true,
            opt_out_notification_methods: Vec::new(),
            host_extensions: Vec::new(),
            channel_capacity: 8,
        }
    }

    #[tokio::test]
    async fn typed_request_roundtrip_works() {
        let client = start_test_client(SessionSource::Exec).await;
        let _response: ConfigRequirementsReadResponse = client
            .request_typed(ClientRequest::ConfigRequirementsRead {
                request_id: RequestId::Integer(1),
                params: None,
            })
            .await
            .expect("typed request should succeed");
        client.shutdown().await.expect("shutdown should complete");
    }

    #[tokio::test]
    async fn typed_request_reports_json_rpc_errors() {
        let client = start_test_client(SessionSource::Exec).await;
        let err = client
            .request_typed::<ConfigRequirementsReadResponse>(ClientRequest::ThreadRead {
                request_id: RequestId::Integer(99),
                params: praxis_app_gateway_protocol::ThreadReadParams {
                    thread_id: "missing-thread".to_string(),
                    include_turns: false,
                },
            })
            .await
            .expect_err("missing thread should return a JSON-RPC error");
        assert!(
            err.to_string().starts_with("thread/read failed:"),
            "expected method-qualified JSON-RPC failure message"
        );
        client.shutdown().await.expect("shutdown should complete");
    }

    #[tokio::test]
    async fn caller_provided_session_source_is_applied() {
        for (session_source, expected_source) in [
            (SessionSource::Exec, ApiSessionSource::Exec),
            (SessionSource::Cli, ApiSessionSource::Cli),
        ] {
            let client = start_test_client(session_source).await;
            let parsed: ThreadStartResponse = client
                .request_typed(ClientRequest::ThreadStart {
                    request_id: RequestId::Integer(2),
                    params: ThreadStartParams {
                        ephemeral: Some(true),
                        ..ThreadStartParams::default()
                    },
                })
                .await
                .expect("thread/start should succeed");
            assert_eq!(parsed.thread.source, expected_source);
            client.shutdown().await.expect("shutdown should complete");
        }
    }

    #[tokio::test]
    async fn threads_started_via_app_gateway_are_visible_through_typed_requests() {
        let client = start_test_client(SessionSource::Cli).await;

        let response: ThreadStartResponse = client
            .request_typed(ClientRequest::ThreadStart {
                request_id: RequestId::Integer(3),
                params: ThreadStartParams {
                    ephemeral: Some(true),
                    ..ThreadStartParams::default()
                },
            })
            .await
            .expect("thread/start should succeed");
        let read = client
            .request_typed::<praxis_app_gateway_protocol::ThreadReadResponse>(
                ClientRequest::ThreadRead {
                    request_id: RequestId::Integer(4),
                    params: praxis_app_gateway_protocol::ThreadReadParams {
                        thread_id: response.thread.id.clone(),
                        include_turns: false,
                    },
                },
            )
            .await
            .expect("thread/read should return the newly started thread");
        assert_eq!(read.thread.id, response.thread.id);

        client.shutdown().await.expect("shutdown should complete");
    }

    #[tokio::test]
    async fn tiny_channel_capacity_still_supports_request_roundtrip() {
        let client =
            start_test_client_with_capacity(SessionSource::Exec, /*channel_capacity*/ 1).await;
        let _response: ConfigRequirementsReadResponse = client
            .request_typed(ClientRequest::ConfigRequirementsRead {
                request_id: RequestId::Integer(1),
                params: None,
            })
            .await
            .expect("typed request should succeed");
        client.shutdown().await.expect("shutdown should complete");
    }

    #[tokio::test]
    async fn forward_in_process_event_preserves_transcript_notifications_under_backpressure() {
        let (event_tx, mut event_rx) = mpsc::channel(1);
        event_tx
            .send(NativeGatewayEvent::Notification(
                command_execution_output_delta_notification("stdout-1"),
            ))
            .await
            .expect("initial event should enqueue");

        let mut skipped_events = 0usize;
        let result = forward_in_process_event(
            &event_tx,
            &mut skipped_events,
            NativeGatewayEvent::Notification(command_execution_output_delta_notification(
                "stdout-2",
            )),
            |_| {},
        )
        .await;
        assert_eq!(result, ForwardEventResult::Continue);
        assert_eq!(skipped_events, 1);

        let receive_task = tokio::spawn(async move {
            let mut events = Vec::new();
            for _ in 0..5 {
                events.push(
                    timeout(Duration::from_secs(2), event_rx.recv())
                        .await
                        .expect("event should arrive before timeout")
                        .expect("event stream should stay open"),
                );
            }
            events
        });

        for notification in [
            agent_message_delta_notification("hello"),
            item_completed_notification("hello"),
            turn_completed_notification(),
        ] {
            let result = forward_in_process_event(
                &event_tx,
                &mut skipped_events,
                NativeGatewayEvent::Notification(notification),
                |_| {},
            )
            .await;
            assert_eq!(result, ForwardEventResult::Continue);
        }
        assert_eq!(skipped_events, 0);

        let events = receive_task
            .await
            .expect("receiver task should join successfully");
        assert!(matches!(
            &events[0],
            NativeGatewayEvent::Notification(
                ServerNotification::CommandExecutionOutputDelta(notification)
            ) if notification.delta == "stdout-1"
        ));
        assert!(matches!(
            &events[1],
            NativeGatewayEvent::Lagged { skipped: 1 }
        ));
        assert!(matches!(
            &events[2],
            NativeGatewayEvent::Notification(ServerNotification::AgentMessageDelta(
                notification
            )) if notification.delta == "hello"
        ));
        assert!(matches!(
            &events[3],
            NativeGatewayEvent::Notification(ServerNotification::ItemCompleted(
                notification
            )) if matches!(
                &notification.item,
                praxis_app_gateway_protocol::ThreadItem::AgentMessage { text, .. } if text == "hello"
            )
        ));
        assert!(matches!(
            &events[4],
            NativeGatewayEvent::Notification(ServerNotification::TurnCompleted(
                notification
            )) if notification.turn.status == praxis_app_gateway_protocol::TurnStatus::Completed
        ));
    }

    #[tokio::test]
    async fn remote_typed_request_roundtrip_works() {
        let websocket_url = start_test_remote_server(|mut websocket| async move {
            expect_remote_initialize(&mut websocket).await;
            let JSONRPCMessage::Request(request) = read_websocket_message(&mut websocket).await
            else {
                panic!("expected account/read request");
            };
            assert_eq!(request.method, "account/read");
            write_websocket_message(
                &mut websocket,
                JSONRPCMessage::Response(JSONRPCResponse {
                    id: request.id,
                    result: serde_json::to_value(GetAccountResponse {
                        account: None,
                        requires_openai_auth: false,
                    })
                    .expect("response should serialize"),
                }),
            )
            .await;
            websocket.close(None).await.expect("close should succeed");
        })
        .await;
        let client = RemoteAppGatewayClient::connect(test_remote_connect_args(websocket_url))
            .await
            .expect("remote client should connect");

        let response: GetAccountResponse = client
            .request_typed(ClientRequest::GetAccount {
                request_id: RequestId::Integer(1),
                params: praxis_app_gateway_protocol::GetAccountParams {
                    refresh_token: false,
                },
            })
            .await
            .expect("typed request should succeed");
        assert_eq!(response.account, None);

        client.shutdown().await.expect("shutdown should complete");
    }

    #[tokio::test]
    async fn remote_connect_includes_auth_header_when_configured() {
        let auth_token = "remote-bearer-token".to_string();
        let websocket_url = start_test_remote_server_with_auth(
            Some(auth_token.clone()),
            |mut websocket| async move {
                expect_remote_initialize(&mut websocket).await;
                websocket.close(None).await.expect("close should succeed");
            },
        )
        .await;
        let client = RemoteAppGatewayClient::connect(RemoteAppGatewayConnectArgs {
            auth_token: Some(auth_token),
            ..test_remote_connect_args(websocket_url)
        })
        .await
        .expect("remote client should connect");

        client.shutdown().await.expect("shutdown should complete");
    }

    #[tokio::test]
    async fn remote_connect_rejects_non_loopback_ws_when_auth_configured() {
        let result = RemoteAppGatewayClient::connect(RemoteAppGatewayConnectArgs {
            websocket_url: "ws://example.com:4500".to_string(),
            auth_token: Some("remote-bearer-token".to_string()),
            ..test_remote_connect_args("ws://127.0.0.1:1".to_string())
        })
        .await;
        let err = match result {
            Ok(_) => panic!("non-loopback ws should be rejected before connect"),
            Err(err) => err,
        };
        assert_eq!(err.kind(), ErrorKind::InvalidInput);
        assert!(
            err.to_string()
                .contains("remote auth tokens require `wss://` or loopback `ws://` URLs")
        );
    }

    #[test]
    fn remote_auth_token_transport_policy_allows_wss_and_loopback_ws() {
        assert!(crate::remote::websocket_url_supports_auth_token(
            &url::Url::parse("wss://example.com:443").expect("wss URL should parse")
        ));
        assert!(crate::remote::websocket_url_supports_auth_token(
            &url::Url::parse("ws://127.0.0.1:4500").expect("loopback ws URL should parse")
        ));
        assert!(!crate::remote::websocket_url_supports_auth_token(
            &url::Url::parse("ws://example.com:4500").expect("non-loopback ws URL should parse")
        ));
    }

    #[tokio::test]
    async fn remote_duplicate_request_id_keeps_original_waiter() {
        let (first_request_seen_tx, first_request_seen_rx) = tokio::sync::oneshot::channel();
        let websocket_url = start_test_remote_server(|mut websocket| async move {
            expect_remote_initialize(&mut websocket).await;
            let JSONRPCMessage::Request(request) = read_websocket_message(&mut websocket).await
            else {
                panic!("expected account/read request");
            };
            assert_eq!(request.method, "account/read");
            first_request_seen_tx
                .send(request.id.clone())
                .expect("request id should send");
            assert!(
                timeout(
                    Duration::from_millis(100),
                    read_websocket_message(&mut websocket)
                )
                .await
                .is_err(),
                "duplicate request should not be forwarded to the server"
            );
            write_websocket_message(
                &mut websocket,
                JSONRPCMessage::Response(JSONRPCResponse {
                    id: request.id,
                    result: serde_json::to_value(GetAccountResponse {
                        account: None,
                        requires_openai_auth: false,
                    })
                    .expect("response should serialize"),
                }),
            )
            .await;
            let _ = websocket.next().await;
        })
        .await;
        let client = RemoteAppGatewayClient::connect(test_remote_connect_args(websocket_url))
            .await
            .expect("remote client should connect");
        let first_request_handle = client.request_handle();
        let second_request_handle = first_request_handle.clone();

        let first_request = tokio::spawn(async move {
            first_request_handle
                .request_typed::<GetAccountResponse>(ClientRequest::GetAccount {
                    request_id: RequestId::Integer(1),
                    params: praxis_app_gateway_protocol::GetAccountParams {
                        refresh_token: false,
                    },
                })
                .await
        });

        let first_request_id = first_request_seen_rx
            .await
            .expect("server should observe the first request");
        assert_eq!(first_request_id, RequestId::Integer(1));

        let second_err = second_request_handle
            .request_typed::<GetAccountResponse>(ClientRequest::GetAccount {
                request_id: RequestId::Integer(1),
                params: praxis_app_gateway_protocol::GetAccountParams {
                    refresh_token: false,
                },
            })
            .await
            .expect_err("duplicate request id should be rejected");
        assert_eq!(
            second_err.to_string(),
            "account/read transport error: duplicate remote app-gateway request id `1`"
        );

        let first_response = first_request
            .await
            .expect("first request task should join")
            .expect("first request should succeed");
        assert_eq!(
            first_response,
            GetAccountResponse {
                account: None,
                requires_openai_auth: false,
            }
        );

        client.shutdown().await.expect("shutdown should complete");
    }

    #[tokio::test]
    async fn remote_notifications_arrive_over_websocket() {
        let websocket_url = start_test_remote_server(|mut websocket| async move {
            expect_remote_initialize(&mut websocket).await;
            write_websocket_message(
                &mut websocket,
                JSONRPCMessage::Notification(
                    serde_json::from_value(
                        serde_json::to_value(ServerNotification::AccountUpdated(
                            AccountUpdatedNotification {
                                auth_mode: None,
                                plan_type: None,
                            },
                        ))
                        .expect("notification should serialize"),
                    )
                    .expect("notification should convert to JSON-RPC"),
                ),
            )
            .await;
        })
        .await;
        let mut client = RemoteAppGatewayClient::connect(test_remote_connect_args(websocket_url))
            .await
            .expect("remote client should connect");

        let event = client.next_event().await.expect("event should arrive");
        assert!(matches!(
            event,
            AppGatewayEvent::ServerNotification(ServerNotification::AccountUpdated(_))
        ));

        client.shutdown().await.expect("shutdown should complete");
    }

    #[tokio::test]
    async fn remote_backpressure_preserves_transcript_notifications() {
        let (done_tx, done_rx) = tokio::sync::oneshot::channel();
        let websocket_url = start_test_remote_server(|mut websocket| async move {
            expect_remote_initialize(&mut websocket).await;
            for notification in [
                command_execution_output_delta_notification("stdout-1"),
                command_execution_output_delta_notification("stdout-2"),
                agent_message_delta_notification("hello"),
                item_completed_notification("hello"),
                turn_completed_notification(),
            ] {
                write_websocket_message(
                    &mut websocket,
                    JSONRPCMessage::Notification(
                        serde_json::from_value(
                            serde_json::to_value(notification)
                                .expect("notification should serialize"),
                        )
                        .expect("notification should convert to JSON-RPC"),
                    ),
                )
                .await;
            }
            let _ = done_rx.await;
        })
        .await;
        let mut client = RemoteAppGatewayClient::connect(RemoteAppGatewayConnectArgs {
            websocket_url,
            auth_token: None,
            client_name: "praxis-app-gateway-client-test".to_string(),
            client_version: "0.0.0-test".to_string(),
            experimental_api: true,
            opt_out_notification_methods: Vec::new(),
            host_extensions: Vec::new(),
            channel_capacity: 1,
        })
        .await
        .expect("remote client should connect");

        let first_event = timeout(Duration::from_secs(2), client.next_event())
            .await
            .expect("first event should arrive before timeout")
            .expect("event stream should stay open");
        assert!(matches!(
            first_event,
            AppGatewayEvent::ServerNotification(ServerNotification::CommandExecutionOutputDelta(
                notification
            )) if notification.delta == "stdout-1"
        ));

        let mut remaining_events = Vec::new();
        for _ in 0..4 {
            remaining_events.push(
                timeout(Duration::from_secs(2), client.next_event())
                    .await
                    .expect("event should arrive before timeout")
                    .expect("event stream should stay open"),
            );
        }

        let mut transcript_event_names = Vec::new();
        for event in &remaining_events {
            match event {
                AppGatewayEvent::Lagged { skipped: 1 } => {}
                AppGatewayEvent::ServerNotification(
                    ServerNotification::CommandExecutionOutputDelta(notification),
                ) if notification.delta == "stdout-2" => {}
                AppGatewayEvent::ServerNotification(ServerNotification::AgentMessageDelta(
                    notification,
                )) if notification.delta == "hello" => {
                    transcript_event_names.push("agent_message_delta");
                }
                AppGatewayEvent::ServerNotification(ServerNotification::ItemCompleted(
                    notification,
                )) if matches!(
                    &notification.item,
                    praxis_app_gateway_protocol::ThreadItem::AgentMessage { text, .. } if text == "hello"
                ) =>
                {
                    transcript_event_names.push("item_completed");
                }
                AppGatewayEvent::ServerNotification(ServerNotification::TurnCompleted(
                    notification,
                )) if notification.turn.status
                    == praxis_app_gateway_protocol::TurnStatus::Completed =>
                {
                    transcript_event_names.push("turn_completed");
                }
                _ => panic!("unexpected remaining event: {event:?}"),
            }
        }
        assert_eq!(
            transcript_event_names,
            vec!["agent_message_delta", "item_completed", "turn_completed"]
        );

        done_tx
            .send(())
            .expect("server completion signal should send");
        client.shutdown().await.expect("shutdown should complete");
    }

    #[tokio::test]
    async fn remote_server_request_resolution_roundtrip_works() {
        let websocket_url = start_test_remote_server(|mut websocket| async move {
            expect_remote_initialize(&mut websocket).await;
            let request_id = RequestId::String("srv-1".to_string());
            let server_request = JSONRPCRequest {
                id: request_id.clone(),
                method: "item/tool/requestUserInput".to_string(),
                params: Some(
                    serde_json::to_value(ToolRequestUserInputParams {
                        thread_id: "thread-1".to_string(),
                        turn_id: "turn-1".to_string(),
                        item_id: "call-1".to_string(),
                        questions: vec![ToolRequestUserInputQuestion {
                            id: "question-1".to_string(),
                            header: "Mode".to_string(),
                            question: "Pick one".to_string(),
                            is_other: false,
                            is_secret: false,
                            options: Some(vec![]),
                        }],
                    })
                    .expect("params should serialize"),
                ),
                trace: None,
            };
            write_websocket_message(&mut websocket, JSONRPCMessage::Request(server_request)).await;

            let JSONRPCMessage::Response(response) = read_websocket_message(&mut websocket).await
            else {
                panic!("expected server request response");
            };
            assert_eq!(response.id, request_id);
        })
        .await;
        let mut client = RemoteAppGatewayClient::connect(test_remote_connect_args(websocket_url))
            .await
            .expect("remote client should connect");

        let AppGatewayEvent::ServerRequest(request) = client
            .next_event()
            .await
            .expect("request event should arrive")
        else {
            panic!("expected server request event");
        };
        client
            .resolve_server_request(request.id().clone(), serde_json::json!({}))
            .await
            .expect("server request should resolve");

        client.shutdown().await.expect("shutdown should complete");
    }

    #[tokio::test]
    async fn remote_server_request_received_during_initialize_is_delivered() {
        let websocket_url = start_test_remote_server(|mut websocket| async move {
            let JSONRPCMessage::Request(request) = read_websocket_message(&mut websocket).await
            else {
                panic!("expected initialize request");
            };
            assert_eq!(request.method, "initialize");

            let request_id = RequestId::String("srv-init".to_string());
            write_websocket_message(
                &mut websocket,
                JSONRPCMessage::Request(JSONRPCRequest {
                    id: request_id.clone(),
                    method: "item/tool/requestUserInput".to_string(),
                    params: Some(
                        serde_json::to_value(ToolRequestUserInputParams {
                            thread_id: "thread-1".to_string(),
                            turn_id: "turn-1".to_string(),
                            item_id: "call-1".to_string(),
                            questions: vec![ToolRequestUserInputQuestion {
                                id: "question-1".to_string(),
                                header: "Mode".to_string(),
                                question: "Pick one".to_string(),
                                is_other: false,
                                is_secret: false,
                                options: Some(vec![]),
                            }],
                        })
                        .expect("params should serialize"),
                    ),
                    trace: None,
                }),
            )
            .await;
            write_websocket_message(
                &mut websocket,
                JSONRPCMessage::Response(JSONRPCResponse {
                    id: request.id,
                    result: serde_json::json!({}),
                }),
            )
            .await;

            let JSONRPCMessage::Notification(notification) =
                read_websocket_message(&mut websocket).await
            else {
                panic!("expected initialized notification");
            };
            assert_eq!(notification.method, "initialized");

            let JSONRPCMessage::Response(response) = read_websocket_message(&mut websocket).await
            else {
                panic!("expected server request response");
            };
            assert_eq!(response.id, request_id);
        })
        .await;
        let mut client = RemoteAppGatewayClient::connect(test_remote_connect_args(websocket_url))
            .await
            .expect("remote client should connect");

        let AppGatewayEvent::ServerRequest(request) = client
            .next_event()
            .await
            .expect("request event should arrive")
        else {
            panic!("expected server request event");
        };
        client
            .resolve_server_request(request.id().clone(), serde_json::json!({}))
            .await
            .expect("server request should resolve");

        client.shutdown().await.expect("shutdown should complete");
    }

    #[tokio::test]
    async fn remote_unknown_server_request_is_rejected() {
        let websocket_url = start_test_remote_server(|mut websocket| async move {
            expect_remote_initialize(&mut websocket).await;
            let request_id = RequestId::String("srv-unknown".to_string());
            write_websocket_message(
                &mut websocket,
                JSONRPCMessage::Request(JSONRPCRequest {
                    id: request_id.clone(),
                    method: "thread/unknown".to_string(),
                    params: None,
                    trace: None,
                }),
            )
            .await;

            let JSONRPCMessage::Error(response) = read_websocket_message(&mut websocket).await
            else {
                panic!("expected JSON-RPC error response");
            };
            assert_eq!(response.id, request_id);
            assert_eq!(response.error.code, -32601);
            assert_eq!(
                response.error.message,
                "unsupported remote app-gateway request `thread/unknown`"
            );
        })
        .await;
        let client = RemoteAppGatewayClient::connect(test_remote_connect_args(websocket_url))
            .await
            .expect("remote client should connect");

        client.shutdown().await.expect("shutdown should complete");
    }

    #[tokio::test]
    async fn remote_disconnect_surfaces_as_event() {
        let websocket_url = start_test_remote_server(|mut websocket| async move {
            expect_remote_initialize(&mut websocket).await;
            websocket.close(None).await.expect("close should succeed");
        })
        .await;
        let mut client = RemoteAppGatewayClient::connect(test_remote_connect_args(websocket_url))
            .await
            .expect("remote client should connect");

        let event = client
            .next_event()
            .await
            .expect("disconnect event should arrive");
        assert!(matches!(event, AppGatewayEvent::Disconnected { .. }));
    }

    #[test]
    fn typed_request_error_exposes_sources() {
        let transport = TypedRequestError::Transport {
            method: "config/read".to_string(),
            source: IoError::new(ErrorKind::BrokenPipe, "closed"),
        };
        assert_eq!(std::error::Error::source(&transport).is_some(), true);

        let server = TypedRequestError::Server {
            method: "thread/read".to_string(),
            source: JSONRPCErrorError {
                code: -32603,
                data: None,
                message: "internal".to_string(),
            },
        };
        assert_eq!(std::error::Error::source(&server).is_some(), false);

        let deserialize = TypedRequestError::Deserialize {
            method: "thread/start".to_string(),
            source: serde_json::from_str::<u32>("\"nope\"")
                .expect_err("invalid integer should return deserialize error"),
        };
        assert_eq!(std::error::Error::source(&deserialize).is_some(), true);
    }

    #[tokio::test]
    async fn next_event_surfaces_lagged_markers() {
        let (command_tx, _command_rx) = mpsc::channel(1);
        let (event_tx, event_rx) = mpsc::channel(1);
        let worker_handle = tokio::spawn(async {});
        event_tx
            .send(NativeGatewayEvent::Lagged { skipped: 3 })
            .await
            .expect("lagged marker should enqueue");
        drop(event_tx);

        let mut client = NativeAppGatewayClient {
            command_tx,
            event_rx,
            worker_handle,
        };

        let event = timeout(Duration::from_secs(2), client.next_event())
            .await
            .expect("lagged marker should arrive before timeout");
        assert!(matches!(
            event,
            Some(NativeGatewayEvent::Lagged { skipped: 3 })
        ));

        client.shutdown().await.expect("shutdown should complete");
    }

    #[test]
    fn event_requires_delivery_marks_transcript_and_terminal_events() {
        assert!(event_requires_delivery(&NativeGatewayEvent::Notification(
            praxis_app_gateway_protocol::ServerNotification::TurnCompleted(
                praxis_app_gateway_protocol::TurnCompletedNotification {
                    thread_id: "thread".to_string(),
                    turn: praxis_app_gateway_protocol::Turn {
                        id: "turn".to_string(),
                        items: Vec::new(),
                        status: praxis_app_gateway_protocol::TurnStatus::Completed,
                        error: None,
                    },
                }
            )
        )));
        assert!(event_requires_delivery(&NativeGatewayEvent::Notification(
            praxis_app_gateway_protocol::ServerNotification::AgentMessageDelta(
                praxis_app_gateway_protocol::AgentMessageDeltaNotification {
                    thread_id: "thread".to_string(),
                    turn_id: "turn".to_string(),
                    item_id: "item".to_string(),
                    delta: "hello".to_string(),
                }
            )
        )));
        assert!(event_requires_delivery(&NativeGatewayEvent::Notification(
            praxis_app_gateway_protocol::ServerNotification::ItemCompleted(
                praxis_app_gateway_protocol::ItemCompletedNotification {
                    thread_id: "thread".to_string(),
                    turn_id: "turn".to_string(),
                    item: praxis_app_gateway_protocol::ThreadItem::AgentMessage {
                        id: "item".to_string(),
                        text: "hello".to_string(),
                        phase: None,
                        memory_citation: None,
                    },
                }
            )
        )));
        assert!(!event_requires_delivery(&NativeGatewayEvent::Lagged {
            skipped: 1
        }));
        assert!(!event_requires_delivery(&NativeGatewayEvent::Notification(
            praxis_app_gateway_protocol::ServerNotification::CommandExecutionOutputDelta(
                praxis_app_gateway_protocol::CommandExecutionOutputDeltaNotification {
                    thread_id: "thread".to_string(),
                    turn_id: "turn".to_string(),
                    item_id: "item".to_string(),
                    delta: "stdout".to_string(),
                }
            )
        )));
    }

    #[tokio::test]
    async fn runtime_start_args_leave_manager_bootstrap_to_app_gateway() {
        let config = Arc::new(build_test_config().await);

        let runtime_args = NativeAppGatewayClientStartArgs {
            arg0_paths: Arg0DispatchPaths::default(),
            config: config.clone(),
            cli_overrides: Vec::new(),
            loader_overrides: LoaderOverrides::default(),
            cloud_requirements: CloudConfigBundleLoader::default(),
            feedback: PraxisFeedback::new(),
            config_warnings: Vec::new(),
            session_source: SessionSource::Exec,
            enable_praxis_api_key_env: false,
            client_name: "praxis-app-gateway-client-test".to_string(),
            client_version: "0.0.0-test".to_string(),
            experimental_api: true,
            opt_out_notification_methods: Vec::new(),
            host_extensions: Vec::new(),
            channel_capacity: DEFAULT_NATIVE_GATEWAY_CHANNEL_CAPACITY,
            control_listen: None,
            control_auth: NativeControlAuthSettings::default(),
        }
        .into_runtime_start_args();

        assert_eq!(runtime_args.config, config);
    }

    #[tokio::test]
    async fn shutdown_completes_promptly_without_retained_managers() {
        let client = start_test_client(SessionSource::Cli).await;

        timeout(Duration::from_secs(1), client.shutdown())
            .await
            .expect("shutdown should not wait for the 5s fallback timeout")
            .expect("shutdown should complete");
    }
}
