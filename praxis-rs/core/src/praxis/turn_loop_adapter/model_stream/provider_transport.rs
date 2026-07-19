//! Provider stream opening, retry, failover, and transport errors.

#![allow(unused_imports)]

use super::*;

pub(in crate::praxis::turn_loop_adapter::model_stream) mod provider_stream {
    use async_stream::try_stream;
    use praxis_loop::services::ModelEventStream;
    use tokio_util::sync::CancellationToken;

    use super::stream_item_state::StreamItemState;
    use crate::client_common::Prompt;

    use self::retrying::DriverOpenStep;
    use self::retrying::EventReadStep;
    use super::PraxisModelStreamInput;
    use super::provider_projection;
    use super::provider_projection::ProviderStreamStep;
    use super::stream_run_state::ProviderStreamRunState;
    use crate::tools::code_mode::CodeModeTurnWorker;

    mod driver {

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

        mod open {

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

            pub(in crate::praxis::turn_loop_adapter::model_stream) async fn open_response_stream(
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
        }

        mod receive {

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

            pub(in crate::praxis::turn_loop_adapter::model_stream) async fn read_response_event(
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

                    Err(praxis_async_utils::CancelErr::Cancelled) => {
                        return Err(PraxisErr::TurnAborted);
                    }
                };

                match event {
                    Some(res) => res,

                    None => Err(PraxisErr::Stream(
                        "stream closed before response.completed".into(),
                        None,
                    )),
                }
            }
        }

        pub(in crate::praxis::turn_loop_adapter::model_stream) struct ProviderStreamDriver {
            stream: ResponseStream,

            receiving_span: tracing::Span,
        }

        pub(in crate::praxis::turn_loop_adapter::model_stream) struct ReceivedResponseEvent {
            pub(in crate::praxis::turn_loop_adapter::model_stream) event: ResponseEvent,
        }

        impl ProviderStreamDriver {
            pub(in crate::praxis::turn_loop_adapter::model_stream) async fn open(
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

            pub(in crate::praxis::turn_loop_adapter::model_stream) async fn next_event(
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
    }
    mod opening {
        use tokio_util::sync::CancellationToken;

        use crate::client_common::Prompt;
        use crate::error::PraxisErr;

        use super::super::PraxisModelStreamInput;
        use super::super::request_telemetry::record_model_request_start;
        use super::driver::ProviderStreamDriver;

        pub(in crate::praxis::turn_loop_adapter::model_stream) async fn open_driver(
            input: &PraxisModelStreamInput,
            prompt: &Prompt,
            turn_metadata_header: Option<&str>,
            cancellation_token: &CancellationToken,
        ) -> Result<ProviderStreamDriver, PraxisErr> {
            record_model_request_start(input.session.as_ref(), input.turn_context.as_ref());
            let mut runtime_state = input.runtime_state.lock().await;
            ProviderStreamDriver::open(
                runtime_state.client_session_mut(),
                &input.turn_context,
                prompt,
                turn_metadata_header,
                cancellation_token,
            )
            .await
        }
    }
    mod retry {
        use praxis_loop::outcome::LoopResult;
        use tokio_util::sync::CancellationToken;
        use tracing::warn;

        use crate::error::PraxisErr;
        use crate::util::backoff;

        use super::super::PraxisModelStreamInput;
        use super::super::error_bridge::finish_model_error;
        use super::super::stream_run_state::ModelStreamProgress;
        use super::retry_notice;
        use super::transport_failover;

        pub(in crate::praxis::turn_loop_adapter::model_stream) async fn wait_before_retry_or_error(
            input: &PraxisModelStreamInput,
            err: PraxisErr,
            cancellation_token: &CancellationToken,
            retries: &mut u64,
            progress: ModelStreamProgress,
        ) -> LoopResult<()> {
            if progress.has_model_output() || !err.is_retryable() {
                return Err(finish_model_error(input, err).await);
            }

            let max_retries = input.turn_context.provider.stream_max_retries();
            if *retries >= max_retries && transport_failover::switch_to_http_transport(input).await
            {
                transport_failover::warn_http_transport_failover(input, &err).await;
                *retries = 0;
                return Ok(());
            }

            if *retries < max_retries {
                *retries += 1;
                let delay = retry_delay(&err, *retries);
                if matches!(err, PraxisErr::ProviderRateLimited(_)) {
                    crate::llm::runtime::provider_coordination::observe_rate_limit(
                        &input.turn_context.config.model_provider_id,
                        input.turn_context.provider.base_url.as_deref(),
                        delay,
                    )
                    .await;
                }
                warn!(
                    "stream disconnected - retrying model request ({retries}/{max_retries} in {delay:?})...",
                );

                retry_notice::maybe_notify_retry(input, *retries, max_retries, err).await;
                tokio::select! {
                    _ = tokio::time::sleep(delay) => {}
                    _ = cancellation_token.cancelled() => {
                        return Err(finish_model_error(input, PraxisErr::TurnAborted).await);
                    }
                }
                return Ok(());
            }

            Err(finish_model_error(input, err).await)
        }

        fn retry_delay(err: &PraxisErr, retries: u64) -> std::time::Duration {
            match err {
                PraxisErr::Stream(_, requested_delay) => {
                    requested_delay.unwrap_or_else(|| backoff(retries))
                }
                PraxisErr::ProviderRateLimited(rate_limit) => {
                    rate_limit.retry_after.unwrap_or_else(|| backoff(retries))
                }
                _ => backoff(retries),
            }
        }
    }
    mod retry_notice {
        use crate::error::PraxisErr;

        use super::super::PraxisModelStreamInput;

        pub(in crate::praxis::turn_loop_adapter::model_stream) async fn maybe_notify_retry(
            input: &PraxisModelStreamInput,
            retries: u64,
            max_retries: u64,
            err: PraxisErr,
        ) {
            if !should_report_retry(input, retries) {
                return;
            }

            input
                .session
                .notify_stream_error(
                    &input.turn_context,
                    format!("Reconnecting... {retries}/{max_retries}"),
                    err,
                )
                .await;
        }

        fn should_report_retry(input: &PraxisModelStreamInput, retries: u64) -> bool {
            retries > 1
                || cfg!(debug_assertions)
                || !input
                    .session
                    .services
                    .model_runtime
                    .responses_websocket_enabled_for(
                        &input.turn_context.config.model_provider_id,
                        &input.turn_context.provider,
                    )
        }
    }
    mod retrying {
        use praxis_loop::outcome::LoopResult;
        use tokio_util::sync::CancellationToken;

        use super::driver::ProviderStreamDriver;
        use super::driver::ReceivedResponseEvent;
        use crate::client_common::Prompt;
        use crate::error::PraxisErr;

        use super::super::PraxisModelStreamInput;
        use super::super::stream_run_state::ProviderStreamRunState;
        use super::opening;
        use super::retry;

        pub(in crate::praxis::turn_loop_adapter::model_stream) enum DriverOpenStep {
            Opened(ProviderStreamDriver),
            RetryAfterWait,
        }

        pub(in crate::praxis::turn_loop_adapter::model_stream) enum EventReadStep {
            Received(ReceivedResponseEvent),
            RetryAfterWait,
        }

        pub(in crate::praxis::turn_loop_adapter::model_stream) async fn open_or_wait_for_retry(
            input: &PraxisModelStreamInput,
            prompt: &Prompt,
            turn_metadata_header: Option<&str>,
            cancellation_token: &CancellationToken,
            run_state: &mut ProviderStreamRunState,
        ) -> LoopResult<DriverOpenStep> {
            if !crate::llm::runtime::provider_coordination::wait_until_ready(
                &input.turn_context.config.model_provider_id,
                input.turn_context.provider.base_url.as_deref(),
                cancellation_token,
            )
            .await
            {
                return Err(super::super::error_bridge::finish_model_error(
                    input,
                    PraxisErr::TurnAborted,
                )
                .await);
            }
            match opening::open_driver(input, prompt, turn_metadata_header, cancellation_token)
                .await
            {
                Ok(driver) => Ok(DriverOpenStep::Opened(driver)),
                Err(err) => {
                    wait_before_retry(input, err, cancellation_token, run_state).await?;
                    Ok(DriverOpenStep::RetryAfterWait)
                }
            }
        }

        pub(in crate::praxis::turn_loop_adapter::model_stream) async fn next_event_or_wait_for_retry(
            input: &PraxisModelStreamInput,
            driver: &mut ProviderStreamDriver,
            cancellation_token: &CancellationToken,
            run_state: &mut ProviderStreamRunState,
        ) -> LoopResult<EventReadStep> {
            match driver
                .next_event(&input.session, &input.turn_context, cancellation_token)
                .await
            {
                Ok(received) => Ok(EventReadStep::Received(received)),
                Err(err) => {
                    wait_before_retry(input, err, cancellation_token, run_state).await?;
                    Ok(EventReadStep::RetryAfterWait)
                }
            }
        }

        async fn wait_before_retry(
            input: &PraxisModelStreamInput,
            err: PraxisErr,
            cancellation_token: &CancellationToken,
            run_state: &mut ProviderStreamRunState,
        ) -> LoopResult<()> {
            let progress = run_state.model_stream_progress();
            retry::wait_before_retry_or_error(
                input,
                err,
                cancellation_token,
                run_state.retry_count_mut(),
                progress,
            )
            .await
        }
    }
    mod transport_failover {
        use crate::error::PraxisErr;

        use super::super::PraxisModelStreamInput;

        pub(in crate::praxis::turn_loop_adapter::model_stream) async fn switch_to_http_transport(
            input: &PraxisModelStreamInput,
        ) -> bool {
            let mut runtime_state = input.runtime_state.lock().await;
            runtime_state
                .client_session_mut()
                .try_switch_http_transport(
                    &input.turn_context.session_telemetry,
                    &input.turn_context.model_info,
                )
        }

        pub(in crate::praxis::turn_loop_adapter::model_stream) async fn warn_http_transport_failover(
            input: &PraxisModelStreamInput,
            err: &PraxisErr,
        ) {
            input
                .session
                .turn_event_emitter(&input.turn_context)
                .warning(format!(
                    "Switching from WebSockets to HTTPS transport. {err:#}"
                ))
                .await;
        }
    }
    pub(in crate::praxis::turn_loop_adapter::model_stream) fn open_event_stream(
        input: PraxisModelStreamInput,
        prompt: Prompt,
        turn_metadata_header: Option<String>,
        cancellation_token: CancellationToken,
        code_mode_worker: Option<CodeModeTurnWorker>,
    ) -> ModelEventStream {
        let stream = try_stream! {
            let input = input;
            let prompt = prompt;
            let turn_metadata_header = turn_metadata_header;
            let cancellation_token = cancellation_token;
            let _code_mode_worker = code_mode_worker;
            let mut run_state = ProviderStreamRunState::default();

            loop {
                let mut driver = match retrying::open_or_wait_for_retry(
                    &input,
                    &prompt,
                    turn_metadata_header.as_deref(),
                    &cancellation_token,
                    &mut run_state,
                )
                .await? {
                    DriverOpenStep::Opened(driver) => driver,
                    DriverOpenStep::RetryAfterWait => continue,
                };
                let mut stream_items = StreamItemState::new(&input.turn_context);

                loop {
                    let received = match retrying::next_event_or_wait_for_retry(
                        &input,
                        &mut driver,
                        &cancellation_token,
                        &mut run_state,
                    )
                    .await? {
                        EventReadStep::Received(received) => received,
                        EventReadStep::RetryAfterWait => break,
                    };

                    let projected = provider_projection::project_response_event(
                        &input,
                        &mut stream_items,
                        received.event,
                    )
                    .await?;
                    run_state.observe_model_output(projected.observed_model_output);

                    match projected.step {
                        ProviderStreamStep::Yield(event) => {
                            yield event;
                        }
                        ProviderStreamStep::Finish(event) => {
                            yield event;
                            return;
                        }
                        ProviderStreamStep::Continue => {}
                    }
                }
            }
        };

        Box::pin(stream)
    }
}

pub(in crate::praxis::turn_loop_adapter::model_stream) mod stream_run_state {
    use super::provider_projection::ModelOutputObservation;

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub(in crate::praxis::turn_loop_adapter::model_stream) enum ModelStreamProgress {
        NoModelOutput,
        ModelOutputStarted,
    }

    impl ModelStreamProgress {
        pub(in crate::praxis::turn_loop_adapter::model_stream) const fn has_model_output(
            self,
        ) -> bool {
            matches!(self, Self::ModelOutputStarted)
        }
    }

    #[derive(Default)]
    pub(in crate::praxis::turn_loop_adapter::model_stream) struct ProviderStreamRunState {
        retries: u64,
        emitted_model_event: bool,
    }

    impl ProviderStreamRunState {
        pub(in crate::praxis::turn_loop_adapter::model_stream) fn retry_count_mut(
            &mut self,
        ) -> &mut u64 {
            &mut self.retries
        }

        pub(in crate::praxis::turn_loop_adapter::model_stream) fn model_stream_progress(
            &self,
        ) -> ModelStreamProgress {
            if self.emitted_model_event {
                ModelStreamProgress::ModelOutputStarted
            } else {
                ModelStreamProgress::NoModelOutput
            }
        }

        pub(in crate::praxis::turn_loop_adapter::model_stream) fn observe_model_output(
            &mut self,
            observation: ModelOutputObservation,
        ) {
            self.emitted_model_event |= observation.as_bool();
        }
    }
}

pub(in crate::praxis::turn_loop_adapter::model_stream) mod error_bridge {
    use praxis_loop::outcome::TurnError;
    use praxis_loop::outcome::TurnErrorKind;

    use crate::error::PraxisErr;

    use super::PraxisModelStreamInput;

    pub(in crate::praxis::turn_loop_adapter::model_stream) async fn finish_model_error(
        input: &PraxisModelStreamInput,
        err: PraxisErr,
    ) -> TurnError {
        match err {
            PraxisErr::TurnAborted => TurnError::cancelled(),
            PraxisErr::ContextWindowExceeded(overflow) => {
                input
                    .session
                    .observe_context_overflow(&input.turn_context, &overflow)
                    .await;
                model_error(PraxisErr::ContextWindowExceeded(overflow))
            }
            PraxisErr::UsageLimitReached(err) => {
                if let Some(rate_limits) = err.rate_limits.clone() {
                    input
                        .session
                        .update_rate_limits(&input.turn_context, *rate_limits)
                        .await;
                }
                model_error(PraxisErr::UsageLimitReached(err))
            }
            err => {
                let error_event = err.to_error_event(/*message_prefix*/ None);
                input
                    .turn_context
                    .tool_loop_guard
                    .record_terminal_model_error(error_event.message.clone());
                input
                    .session
                    .turn_event_emitter(&input.turn_context)
                    .error_event(error_event)
                    .await;
                model_error(err)
            }
        }
    }

    pub(in crate::praxis::turn_loop_adapter::model_stream) fn model_error(
        err: PraxisErr,
    ) -> TurnError {
        TurnError::new(TurnErrorKind::Model, err.to_string())
    }
}

pub(in crate::praxis::turn_loop_adapter::model_stream) mod request_telemetry {
    use crate::feedback_tags;
    use crate::praxis::Session;
    use crate::praxis::TurnContext;

    pub(in crate::praxis::turn_loop_adapter::model_stream) fn record_model_request_start(
        sess: &Session,
        turn_context: &TurnContext,
    ) {
        let permissions = turn_context.effective_permissions();
        feedback_tags!(
            model = turn_context.model_info.slug.clone(),
            approval_policy = permissions.approval_policy.value(),
            sandbox_policy = permissions.sandbox_policy.get(),
            effort = turn_context.reasoning_effort,
            auth_mode = sess.services.auth_manager.auth_mode(),
            features = sess.features.enabled_features(),
        );
    }
}
