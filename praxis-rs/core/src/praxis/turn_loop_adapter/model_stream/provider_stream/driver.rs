use std::sync::Arc;

use tokio_util::sync::CancellationToken;
use tracing::trace_span;

use crate::ResponseStream;
use crate::client::ModelClientSession;
use crate::client_common::Prompt;
use crate::client_common::ResponseEvent;
use crate::error::Result as PraxisResult;
use crate::praxis::Session;
use crate::praxis::TurnContext;

mod open;
mod receive;

pub(super) struct ProviderStreamDriver {
    stream: ResponseStream,
    receiving_span: tracing::Span,
}

pub(super) struct ReceivedResponseEvent {
    pub(super) event: ResponseEvent,
}

impl ProviderStreamDriver {
    pub(super) async fn open(
        client_session: &mut ModelClientSession,
        turn_context: &Arc<TurnContext>,
        prompt: &Prompt,
        turn_metadata_header: Option<&str>,
        cancellation_token: &CancellationToken,
    ) -> PraxisResult<Self> {
        let stream = open::open_response_stream(
            client_session,
            turn_context,
            prompt,
            turn_metadata_header,
            cancellation_token,
        )
        .await?;
        Ok(Self {
            stream,
            receiving_span: trace_span!("receiving_stream"),
        })
    }

    pub(super) async fn next_event(
        &mut self,
        sess: &Arc<Session>,
        turn_context: &Arc<TurnContext>,
        cancellation_token: &CancellationToken,
    ) -> PraxisResult<ReceivedResponseEvent> {
        receive::read_response_event(
            &mut self.stream,
            &self.receiving_span,
            sess,
            turn_context,
            cancellation_token,
        )
        .await
    }
}
