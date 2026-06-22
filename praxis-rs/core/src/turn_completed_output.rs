use std::pin::Pin;
use std::sync::Arc;

use futures::Future;
use praxis_protocol::models::FunctionCallOutputBody;
use praxis_protocol::models::FunctionCallOutputPayload;
use praxis_protocol::models::ResponseInputItem;
use praxis_protocol::models::ResponseItem;
use tokio_util::sync::CancellationToken;
use tracing::instrument;
use tracing::warn;

use crate::error::PraxisErr;
use crate::error::Result;
use crate::function_tool::FunctionCallError;
use crate::praxis::Session;
use crate::praxis::TurnContext;
use crate::tools::router::ToolRouter;
use crate::tools::tool_call_runtime::ToolCallRuntime;
use crate::turn_final_answer::tool_loop_guard_final_item;
use crate::turn_output_items::CompletedResponseItemSink;
use crate::turn_output_items::record_completed_response_item;
use crate::turn_output_items::response_input_to_response_item;

pub(crate) type TurnToolFuture<'f> =
    Pin<Box<dyn Future<Output = Result<ResponseInputItem>> + Send + 'f>>;

#[derive(Default)]
pub(crate) struct CompletedOutputResult {
    pub last_agent_message: Option<String>,
    pub needs_follow_up: bool,
    pub tool_future: Option<TurnToolFuture<'static>>,
}

pub(crate) struct CompletedOutputCtx {
    pub sess: Arc<Session>,
    pub turn_context: Arc<TurnContext>,
    pub tool_runtime: ToolCallRuntime,
    pub cancellation_token: CancellationToken,
}

#[instrument(level = "trace", skip_all)]
pub(crate) async fn handle_completed_output_item(
    ctx: &mut CompletedOutputCtx,
    item: ResponseItem,
    previously_active_item: Option<praxis_protocol::items::TurnItem>,
) -> Result<CompletedOutputResult> {
    let mut output = CompletedOutputResult::default();

    match ToolRouter::build_tool_call(ctx.sess.as_ref(), item.clone()).await {
        Ok(Some(call)) => {
            if ctx
                .turn_context
                .tool_loop_guard
                .should_hide_tool(&call.tool_name)
            {
                warn!(
                    tool_name = call.tool_name.as_str(),
                    "hidden tool call suppressed after tool loop guard intervention"
                );
                let final_item =
                    tool_loop_guard_final_item(Arc::clone(&ctx.sess), call.tool_name.as_str())
                        .await;
                let sink =
                    CompletedResponseItemSink::new(ctx.sess.as_ref(), ctx.turn_context.as_ref());
                output.last_agent_message = sink.emit_and_record(&final_item, None).await;
                return Ok(output);
            }

            let payload_preview = call.payload.log_payload().into_owned();
            tracing::info!(
                thread_id = %ctx.sess.conversation_id,
                "ToolCall: {} {}",
                call.tool_name,
                payload_preview
            );

            record_completed_response_item(ctx.sess.as_ref(), ctx.turn_context.as_ref(), &item)
                .await;

            let cancellation_token = ctx.cancellation_token.child_token();
            let tool_future: TurnToolFuture<'static> = Box::pin(
                ctx.tool_runtime
                    .clone()
                    .handle_tool_call(call, cancellation_token),
            );

            output.needs_follow_up = true;
            output.tool_future = Some(tool_future);
        }
        Ok(None) => {
            let sink = CompletedResponseItemSink::new(ctx.sess.as_ref(), ctx.turn_context.as_ref());
            output.last_agent_message = sink
                .emit_and_record(&item, previously_active_item.as_ref())
                .await;
        }
        Err(FunctionCallError::MissingLocalShellCallId) => {
            let msg = "LocalShellCall without call_id or id";
            ctx.turn_context
                .session_telemetry
                .log_tool_failed("local_shell", msg);
            tracing::error!(msg);

            record_tool_error_response(ctx, &item, msg).await;
            output.needs_follow_up = true;
        }
        Err(FunctionCallError::RespondToModel(message)) => {
            record_tool_error_response(ctx, &item, message).await;
            output.needs_follow_up = true;
        }
        Err(FunctionCallError::Fatal(message)) => {
            return Err(PraxisErr::Fatal(message));
        }
    }

    Ok(output)
}

async fn record_tool_error_response(
    ctx: &CompletedOutputCtx,
    source_item: &ResponseItem,
    message: impl Into<String>,
) {
    let response = ResponseInputItem::FunctionCallOutput {
        call_id: String::new(),
        output: FunctionCallOutputPayload {
            body: FunctionCallOutputBody::Text(message.into()),
            ..Default::default()
        },
    };

    record_completed_response_item(ctx.sess.as_ref(), ctx.turn_context.as_ref(), source_item).await;

    if let Some(response_item) = response_input_to_response_item(&response) {
        ctx.sess
            .record_conversation_items(&ctx.turn_context, std::slice::from_ref(&response_item))
            .await;
    }
}
