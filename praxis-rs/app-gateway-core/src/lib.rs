#![forbid(unsafe_code)]

use std::future::Future;
use std::pin::Pin;

use praxis_app_gateway_protocol::GatewayClientInfo;
use praxis_app_gateway_protocol::GatewayErrorPayload;
use praxis_app_gateway_protocol::GatewayEventEnvelope;
use praxis_app_gateway_protocol::GatewayMode;
use praxis_app_gateway_protocol::GatewayRequestEnvelope;
use praxis_app_gateway_protocol::GatewayResponseEnvelope;
use praxis_app_gateway_protocol::GatewayTransport;
use praxis_app_gateway_protocol::HostExtensionInfo;

pub type GatewayResult<T> = Result<T, GatewayError>;
pub type GatewayDispatchFuture<'a> =
    Pin<Box<dyn Future<Output = GatewayResult<GatewayResponseEnvelope>> + Send + 'a>>;
pub type GatewayEventFuture<'a> = Pin<Box<dyn Future<Output = GatewayResult<()>> + Send + 'a>>;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GatewayConnectionId(String);

impl GatewayConnectionId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GatewaySessionId(String);

impl GatewaySessionId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct GatewayRequestContext {
    pub connection_id: GatewayConnectionId,
    pub session_id: GatewaySessionId,
    pub client_info: GatewayClientInfo,
    pub mode: GatewayMode,
    pub transport: GatewayTransport,
    pub host_extensions: Vec<HostExtensionInfo>,
}

impl GatewayRequestContext {
    pub fn new(
        connection_id: GatewayConnectionId,
        session_id: GatewaySessionId,
        client_info: GatewayClientInfo,
        mode: GatewayMode,
        transport: GatewayTransport,
        host_extensions: Vec<HostExtensionInfo>,
    ) -> Self {
        Self {
            connection_id,
            session_id,
            client_info,
            mode,
            transport,
            host_extensions,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct GatewayError {
    pub code: GatewayErrorCode,
    pub message: String,
    pub data: Option<serde_json::Value>,
}

impl GatewayError {
    pub fn new(
        code: GatewayErrorCode,
        message: impl Into<String>,
        data: Option<serde_json::Value>,
    ) -> Self {
        Self {
            code,
            message: message.into(),
            data,
        }
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(GatewayErrorCode::Internal, message, None)
    }

    pub fn into_payload(self) -> GatewayErrorPayload {
        GatewayErrorPayload {
            code: self.code.as_str().to_string(),
            message: self.message,
            data: self.data,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GatewayErrorCode {
    InvalidRequest,
    Unauthorized,
    NotFound,
    Conflict,
    Internal,
}

impl GatewayErrorCode {
    pub const fn as_str(self) -> &'static str {
        match self {
            GatewayErrorCode::InvalidRequest => "invalidRequest",
            GatewayErrorCode::Unauthorized => "unauthorized",
            GatewayErrorCode::NotFound => "notFound",
            GatewayErrorCode::Conflict => "conflict",
            GatewayErrorCode::Internal => "internal",
        }
    }
}

/// Dispatches canonical App Gateway requests without knowing the transport.
pub trait GatewayRequestDispatcher: Send + Sync {
    fn dispatch(
        &self,
        context: GatewayRequestContext,
        request: GatewayRequestEnvelope,
    ) -> GatewayDispatchFuture<'_>;
}

/// Sends App Gateway events through the caller-owned transport or native queue.
pub trait GatewayEventSink: Send + Sync {
    fn send(&self, event: GatewayEventEnvelope) -> GatewayEventFuture<'_>;
}

/// Lists host extensions available to the current gateway session.
pub trait GatewayHostRegistry: Send + Sync {
    fn extensions(&self) -> Vec<HostExtensionInfo>;

    fn find_extension(&self, id: &str) -> Option<HostExtensionInfo> {
        self.extensions()
            .into_iter()
            .find(|extension| extension.id == id)
    }
}

pub struct GatewayCore<D> {
    dispatcher: D,
}

impl<D> GatewayCore<D>
where
    D: GatewayRequestDispatcher,
{
    pub fn new(dispatcher: D) -> Self {
        Self { dispatcher }
    }

    pub fn dispatcher(&self) -> &D {
        &self.dispatcher
    }

    pub fn dispatch(
        &self,
        context: GatewayRequestContext,
        request: GatewayRequestEnvelope,
    ) -> GatewayDispatchFuture<'_> {
        self.dispatcher.dispatch(context, request)
    }
}
