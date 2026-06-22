use std::sync::Arc;

use futures::StreamExt;
use praxis_async_utils::OrCancelExt;
use tokio_util::sync::CancellationToken;
use tracing::Instrument;
use tracing::field;
use tracing::trace_span;

use crate::ResponseStream;
use crate::error::PraxisErr;
use crate::error::Result as PraxisResult;
use crate::praxis::Session;
use crate::praxis::TurnContext;
use crate::turn_timing::record_turn_ttft_metric;

use super::ReceivedResponseEvent;

pub(super) async fn read_response_event(
    stream: &mut ResponseStream,
    receiving_span: &tracing::Span,
    sess: &Arc<Session>,
    turn_context: &Arc<TurnContext>,
    cancellation_token: &CancellationToken,
) -> PraxisResult<ReceivedResponseEvent> {
    let handle_responses = trace_span!(
        parent: receiving_span,
        "handle_responses",
        otel.name = field::Empty,
        tool_name = field::Empty,
        from = field::Empty,
    );

    let event = receive_next(stream, &handle_responses, cancellation_token).await?;
    sess.services
        .session_telemetry
        .record_responses(&handle_responses, &event);
    record_turn_ttft_metric(turn_context, &event).await;

    Ok(ReceivedResponseEvent { event })
}

async fn receive_next(
    stream: &mut ResponseStream,
    handle_responses: &tracing::Span,
    cancellation_token: &CancellationToken,
) -> PraxisResult<crate::client_common::ResponseEvent> {
    let event = match stream
        .next()
        .instrument(trace_span!(parent: handle_responses, "receiving"))
        .or_cancel(cancellation_token)
        .await
    {
        Ok(event) => event,
        Err(praxis_async_utils::CancelErr::Cancelled) => return Err(PraxisErr::TurnAborted),
    };

    match event {
        Some(res) => res,
        None => Err(PraxisErr::Stream(
            "stream closed before response.completed".into(),
            None,
        )),
    }
}
