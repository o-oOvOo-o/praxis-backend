use std::sync::Arc;

use praxis_async_utils::OrCancelExt;
use tokio_util::sync::CancellationToken;
use tracing::Instrument;
use tracing::trace_span;

use crate::ResponseStream;
use crate::client::ModelClientSession;
use crate::client_common::Prompt;
use crate::error::Result as PraxisResult;
use crate::praxis::TurnContext;

pub(super) async fn open_response_stream(
    client_session: &mut ModelClientSession,
    turn_context: &Arc<TurnContext>,
    prompt: &Prompt,
    turn_metadata_header: Option<&str>,
    cancellation_token: &CancellationToken,
) -> PraxisResult<ResponseStream> {
    client_session
        .stream(
            prompt,
            &turn_context.model_info,
            &turn_context.session_telemetry,
            turn_context.reasoning_effort.clone(),
            turn_context.reasoning_summary,
            turn_context.config.service_tier,
            turn_metadata_header,
        )
        .instrument(trace_span!("stream_request"))
        .or_cancel(cancellation_token)
        .await?
}
