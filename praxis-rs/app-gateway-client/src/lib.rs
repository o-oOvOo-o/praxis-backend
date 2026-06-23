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

mod events;
mod remote;
#[cfg(test)]
mod tests;

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

pub use crate::events::AppGatewayEvent;
pub(crate) use crate::events::{
    ForwardEventResult, forward_in_process_event, server_notification_requires_delivery,
};
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
    /// Whether auth loading should honor the legacy `CODEX_API_KEY` compatibility variable.
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
