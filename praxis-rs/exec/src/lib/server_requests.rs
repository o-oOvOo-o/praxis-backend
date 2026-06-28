use super::*;

pub(super) fn canceled_mcp_server_elicitation_response() -> Result<Value, String> {
    serde_json::to_value(McpServerElicitationRequestResponse {
        action: McpServerElicitationAction::Cancel,
        content: None,
        meta: None,
    })
    .map_err(|err| format!("failed to encode mcp elicitation response: {err}"))
}

pub(super) async fn request_shutdown(
    client: &AppGatewayClient,
    request_ids: &mut RequestIdSequencer,
    thread_id: &str,
) -> Result<(), String> {
    let request = ClientRequest::ThreadUnsubscribe {
        request_id: request_ids.next(),
        params: ThreadUnsubscribeParams {
            thread_id: thread_id.to_string(),
        },
    };
    send_request_with_response::<ThreadUnsubscribeResponse>(client, request, "thread/unsubscribe")
        .await
        .map(|_| ())
}

pub(super) async fn resolve_server_request(
    client: &AppGatewayClient,
    request_id: RequestId,
    value: serde_json::Value,
    method: &str,
) -> Result<(), String> {
    client
        .resolve_server_request(request_id, value)
        .await
        .map_err(|err| format!("failed to resolve `{method}` server request: {err}"))
}

pub(super) async fn reject_server_request(
    client: &AppGatewayClient,
    request_id: RequestId,
    method: &str,
    reason: String,
) -> Result<(), String> {
    client
        .reject_server_request(
            request_id,
            JSONRPCErrorError {
                code: -32000,
                message: reason,
                data: None,
            },
        )
        .await
        .map_err(|err| format!("failed to reject `{method}` server request: {err}"))
}

pub(super) fn server_request_method_name(request: &ServerRequest) -> String {
    serde_json::to_value(request)
        .ok()
        .and_then(|value| {
            value
                .get("method")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned)
        })
        .unwrap_or_else(|| "unknown".to_string())
}

pub(super) async fn handle_server_request(
    client: &AppGatewayClient,
    request: ServerRequest,
    error_seen: &mut bool,
) {
    let method = server_request_method_name(&request);
    let handle_result = match request {
        ServerRequest::McpServerElicitationRequest { request_id, .. } => {
            // Exec auto-cancels elicitation instead of surfacing it
            // interactively. Preserve that behavior for attached subagent
            // threads too so we do not turn a cancel into a decline/error.
            match canceled_mcp_server_elicitation_response() {
                Ok(value) => {
                    resolve_server_request(
                        client,
                        request_id,
                        value,
                        "mcpServer/elicitation/request",
                    )
                    .await
                }
                Err(err) => Err(err),
            }
        }
        ServerRequest::CommandExecutionRequestApproval { request_id, params } => {
            reject_server_request(
                client,
                request_id,
                &method,
                format!(
                    "command execution approval is not supported in exec mode for thread `{}`",
                    params.thread_id
                ),
            )
            .await
        }
        ServerRequest::FileChangeRequestApproval { request_id, params } => {
            reject_server_request(
                client,
                request_id,
                &method,
                format!(
                    "file change approval is not supported in exec mode for thread `{}`",
                    params.thread_id
                ),
            )
            .await
        }
        ServerRequest::ToolRequestUserInput { request_id, params } => {
            reject_server_request(
                client,
                request_id,
                &method,
                format!(
                    "request_user_input is not supported in exec mode for thread `{}`",
                    params.thread_id
                ),
            )
            .await
        }
        ServerRequest::DynamicToolCall { request_id, params } => {
            reject_server_request(
                client,
                request_id,
                &method,
                format!(
                    "dynamic tool calls are not supported in exec mode for thread `{}`",
                    params.thread_id
                ),
            )
            .await
        }
        ServerRequest::ChatgptAuthTokensRefresh { request_id, .. } => {
            reject_server_request(
                client,
                request_id,
                &method,
                "chatgpt auth token refresh is not supported in exec mode".to_string(),
            )
            .await
        }
        ServerRequest::PermissionsRequestApproval { request_id, params } => {
            reject_server_request(
                client,
                request_id,
                &method,
                format!(
                    "permissions approval is not supported in exec mode for thread `{}`",
                    params.thread_id
                ),
            )
            .await
        }
    };

    if let Err(err) = handle_result {
        *error_seen = true;
        warn!("{err}");
    }
}
