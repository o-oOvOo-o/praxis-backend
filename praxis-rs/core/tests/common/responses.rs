#![allow(clippy::unwrap_used)]

use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;

use anyhow::Result;
use base64::Engine;
use futures::SinkExt;
use futures::StreamExt;
use praxis_protocol::models::ContentItem;
use praxis_protocol::models::ResponseItem;
use praxis_protocol::openai_models::ModelsResponse;
use serde_json::Value;
use tokio::net::TcpListener;
use tokio::sync::Notify;
use tokio::sync::oneshot;
use tokio_tungstenite::accept_hdr_async_with_config;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::extensions::ExtensionsConfig;
use tokio_tungstenite::tungstenite::extensions::compression::deflate::DeflateConfig;
use tokio_tungstenite::tungstenite::handshake::server::Request;
use tokio_tungstenite::tungstenite::handshake::server::Response;
use tokio_tungstenite::tungstenite::protocol::WebSocketConfig;
use wiremock::BodyPrintLimit;
use wiremock::Match;
use wiremock::Mock;
use wiremock::MockBuilder;
use wiremock::MockServer;
use wiremock::Respond;
use wiremock::ResponseTemplate;
use wiremock::http::HeaderName;
use wiremock::http::HeaderValue;
use wiremock::matchers::method;
use wiremock::matchers::path_regex;

use crate::test_praxis::ApplyPatchModelOutput;

#[derive(Debug, Clone)]
mod events;
mod models;
mod mounting;
mod request;
mod websocket;

pub use events::*;
pub use models::*;
pub use mounting::*;
pub use request::{ResponseMock, ResponsesRequest};
pub use websocket::*;

use request::decode_body_bytes;
