#![forbid(unsafe_code)]

use std::io::Result as IoResult;
use std::sync::Arc;

use praxis_app_gateway_core::GatewayConnectionId;
use praxis_app_gateway_core::GatewayCore;
use praxis_app_gateway_core::GatewayDispatchFuture;
use praxis_app_gateway_core::GatewayRequestContext;
use praxis_app_gateway_core::GatewayRequestDispatcher;
use praxis_app_gateway_core::GatewaySessionId;
use praxis_app_gateway_protocol::GatewayClientInfo;
use praxis_app_gateway_protocol::GatewayMode;
use praxis_app_gateway_protocol::GatewayRequestEnvelope;
use praxis_app_gateway_protocol::GatewayTransport;
use praxis_app_gateway_protocol::HostExtensionInfo;
use praxis_arg0::Arg0DispatchPaths;
use praxis_core::config::Config;
use praxis_core::config_loader::CloudRequirementsLoader;
use praxis_core::config_loader::LoaderOverrides;
use praxis_feedback::CodexFeedback;
use praxis_protocol::protocol::SessionSource;
use toml::Value as TomlValue;

pub use praxis_app_gateway_runtime::in_process::DEFAULT_NATIVE_GATEWAY_CHANNEL_CAPACITY as DEFAULT_NATIVE_GATEWAY_CHANNEL_CAPACITY;

pub type NativeGatewayRequestResult =
    std::result::Result<praxis_app_gateway_protocol::Result, praxis_app_gateway_protocol::JSONRPCErrorError>;

#[derive(Clone, Debug, PartialEq)]
pub struct NativeGatewayAttachParams {
    pub connection_id: GatewayConnectionId,
    pub session_id: GatewaySessionId,
    pub client_info: GatewayClientInfo,
    pub host_extensions: Vec<HostExtensionInfo>,
}

pub struct NativeGateway<D> {
    core: Arc<GatewayCore<D>>,
}

impl<D> NativeGateway<D>
where
    D: GatewayRequestDispatcher,
{
    pub fn new(dispatcher: D) -> Self {
        Self {
            core: Arc::new(GatewayCore::new(dispatcher)),
        }
    }

    pub fn from_core(core: GatewayCore<D>) -> Self {
        Self {
            core: Arc::new(core),
        }
    }

    pub fn attach(&self, params: NativeGatewayAttachParams) -> NativeGatewayHandle<D> {
        NativeGatewayHandle {
            core: Arc::clone(&self.core),
            context: GatewayRequestContext::new(
                params.connection_id,
                params.session_id,
                params.client_info,
                GatewayMode::Native,
                GatewayTransport::Native,
                params.host_extensions,
            ),
        }
    }
}

#[derive(Clone)]
pub struct NativeRuntimeStartArgs {
    pub arg0_paths: Arg0DispatchPaths,
    pub config: Arc<Config>,
    pub cli_overrides: Vec<(String, TomlValue)>,
    pub loader_overrides: LoaderOverrides,
    pub cloud_requirements: CloudRequirementsLoader,
    pub feedback: CodexFeedback,
    pub config_warnings: Vec<praxis_app_gateway_protocol::ConfigWarningNotification>,
    pub session_source: SessionSource,
    pub enable_praxis_api_key_env: bool,
    pub initialize: praxis_app_gateway_protocol::InitializeParams,
    pub channel_capacity: usize,
}

impl NativeRuntimeStartArgs {
    fn into_current_start_args(self) -> praxis_app_gateway_runtime::in_process::InProcessStartArgs {
        praxis_app_gateway_runtime::in_process::InProcessStartArgs {
            arg0_paths: self.arg0_paths,
            config: self.config,
            cli_overrides: self.cli_overrides,
            loader_overrides: self.loader_overrides,
            cloud_requirements: self.cloud_requirements,
            feedback: self.feedback,
            config_warnings: self.config_warnings,
            session_source: self.session_source,
            enable_praxis_api_key_env: self.enable_praxis_api_key_env,
            initialize: self.initialize,
            channel_capacity: self.channel_capacity,
        }
    }
}

#[derive(Debug, Clone)]
pub enum NativeGatewayEvent {
    Lagged { skipped: usize },
    Notification(praxis_app_gateway_protocol::ServerNotification),
    ServerRequest(praxis_app_gateway_protocol::ServerRequest),
}

impl From<praxis_app_gateway_runtime::in_process::InProcessServerEvent> for NativeGatewayEvent {
    fn from(value: praxis_app_gateway_runtime::in_process::InProcessServerEvent) -> Self {
        match value {
            praxis_app_gateway_runtime::in_process::InProcessServerEvent::Lagged { skipped } => {
                Self::Lagged { skipped }
            }
            praxis_app_gateway_runtime::in_process::InProcessServerEvent::ServerNotification(
                notification,
            ) => Self::Notification(notification),
            praxis_app_gateway_runtime::in_process::InProcessServerEvent::ServerRequest(request) => {
                Self::ServerRequest(request)
            }
        }
    }
}

pub struct NativeRuntimeHandle {
    inner: praxis_app_gateway_runtime::in_process::InProcessClientHandle,
}

impl NativeRuntimeHandle {
    pub async fn request(
        &self,
        request: praxis_app_gateway_protocol::ClientRequest,
    ) -> IoResult<NativeGatewayRequestResult> {
        self.inner.request(request).await
    }

    pub fn notify(&self, notification: praxis_app_gateway_protocol::ClientNotification) -> IoResult<()> {
        self.inner.notify(notification)
    }

    pub fn respond_to_server_request(
        &self,
        request_id: praxis_app_gateway_protocol::RequestId,
        result: praxis_app_gateway_protocol::Result,
    ) -> IoResult<()> {
        self.inner.respond_to_server_request(request_id, result)
    }

    pub fn fail_server_request(
        &self,
        request_id: praxis_app_gateway_protocol::RequestId,
        error: praxis_app_gateway_protocol::JSONRPCErrorError,
    ) -> IoResult<()> {
        self.inner.fail_server_request(request_id, error)
    }

    pub async fn next_event(&mut self) -> Option<NativeGatewayEvent> {
        self.inner.next_event().await.map(Into::into)
    }

    pub async fn shutdown(self) -> IoResult<()> {
        self.inner.shutdown().await
    }

    pub fn sender(&self) -> NativeRuntimeSender {
        NativeRuntimeSender {
            inner: self.inner.sender(),
        }
    }
}

#[derive(Clone)]
pub struct NativeRuntimeSender {
    inner: praxis_app_gateway_runtime::in_process::InProcessClientSender,
}

impl NativeRuntimeSender {
    pub async fn request(
        &self,
        request: praxis_app_gateway_protocol::ClientRequest,
    ) -> IoResult<NativeGatewayRequestResult> {
        self.inner.request(request).await
    }

    pub fn notify(&self, notification: praxis_app_gateway_protocol::ClientNotification) -> IoResult<()> {
        self.inner.notify(notification)
    }

    pub fn respond_to_server_request(
        &self,
        request_id: praxis_app_gateway_protocol::RequestId,
        result: praxis_app_gateway_protocol::Result,
    ) -> IoResult<()> {
        self.inner.respond_to_server_request(request_id, result)
    }

    pub fn fail_server_request(
        &self,
        request_id: praxis_app_gateway_protocol::RequestId,
        error: praxis_app_gateway_protocol::JSONRPCErrorError,
    ) -> IoResult<()> {
        self.inner.fail_server_request(request_id, error)
    }
}

pub async fn start_native_runtime(args: NativeRuntimeStartArgs) -> IoResult<NativeRuntimeHandle> {
    let inner = praxis_app_gateway_runtime::in_process::start(args.into_current_start_args()).await?;
    Ok(NativeRuntimeHandle { inner })
}

impl<D> Clone for NativeGateway<D> {
    fn clone(&self) -> Self {
        Self {
            core: Arc::clone(&self.core),
        }
    }
}

pub struct NativeGatewayHandle<D> {
    core: Arc<GatewayCore<D>>,
    context: GatewayRequestContext,
}

impl<D> NativeGatewayHandle<D>
where
    D: GatewayRequestDispatcher,
{
    pub fn context(&self) -> &GatewayRequestContext {
        &self.context
    }

    pub fn dispatch(&self, request: GatewayRequestEnvelope) -> GatewayDispatchFuture<'_> {
        self.core.dispatch(self.context.clone(), request)
    }
}

impl<D> Clone for NativeGatewayHandle<D> {
    fn clone(&self) -> Self {
        Self {
            core: Arc::clone(&self.core),
            context: self.context.clone(),
        }
    }
}
