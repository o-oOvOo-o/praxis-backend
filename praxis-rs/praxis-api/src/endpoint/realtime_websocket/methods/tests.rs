use super::*;
use crate::endpoint::realtime_websocket::protocol::RealtimeHandoffRequested;
use crate::endpoint::realtime_websocket::protocol::RealtimeTranscriptDelta;
use crate::endpoint::realtime_websocket::protocol::RealtimeTranscriptEntry;
use http::HeaderValue;
use praxis_protocol::protocol::RealtimeInputAudioSpeechStarted;
use praxis_protocol::protocol::RealtimeResponseCancelled;
use pretty_assertions::assert_eq;
use serde_json::Value;
use serde_json::json;
use std::collections::HashMap;
use std::time::Duration;
use tokio::net::TcpListener;
use tokio_tungstenite::accept_async;
use tokio_tungstenite::tungstenite::Message;

#[path = "tests/connection_flow.rs"]
mod connection_flow;
#[path = "tests/parser_and_urls.rs"]
mod parser_and_urls;
#[path = "tests/send_nonblocking.rs"]
mod send_nonblocking;
#[path = "tests/session_updates.rs"]
mod session_updates;
