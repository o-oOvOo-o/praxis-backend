use super::*;
use crate::client_common::RESPONSE_STREAM_CAPACITY;

mod claude;
mod common;
mod think_tags;

pub(super) use claude::*;
pub(super) use common::*;
pub(super) use think_tags::*;

pub(super) fn response_is_sse(response: &reqwest::Response) -> bool {
    response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.to_ascii_lowercase().contains("text/event-stream"))
}

pub(super) fn spawn_claude_sse_stream(
    response: reqwest::Response,
    idle_timeout: Duration,
) -> ResponseStream {
    let (tx_event, rx_event) = mpsc::channel::<Result<ResponseEvent>>(RESPONSE_STREAM_CAPACITY);
    tokio::spawn(process_claude_sse(response, tx_event, idle_timeout));
    ResponseStream { rx_event }
}

pub(super) fn spawn_common_sse_stream(
    response: reqwest::Response,
    idle_timeout: Duration,
    thinking_policy: CommonThinkingPolicy,
) -> ResponseStream {
    let (tx_event, rx_event) = mpsc::channel::<Result<ResponseEvent>>(RESPONSE_STREAM_CAPACITY);
    tokio::spawn(process_common_sse(
        response,
        tx_event,
        idle_timeout,
        thinking_policy,
    ));
    ResponseStream { rx_event }
}

pub(super) async fn send_stream_event(
    tx_event: &mpsc::Sender<Result<ResponseEvent>>,
    event: ResponseEvent,
) -> Result<()> {
    tx_event
        .send(Ok(event))
        .await
        .map_err(|err| PraxisErr::Fatal(format!("failed to emit provider stream event: {err}")))
}
