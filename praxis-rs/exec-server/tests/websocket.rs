#![cfg(unix)]

mod common;

use common::exec_server::exec_server;
use praxis_app_gateway_protocol::JSONRPCError;
use praxis_app_gateway_protocol::JSONRPCMessage;
use praxis_app_gateway_protocol::JSONRPCResponse;
use praxis_exec_server::InitializeParams;
use praxis_exec_server::InitializeResponse;
use pretty_assertions::assert_eq;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn exec_server_reports_malformed_websocket_json_and_keeps_running() -> anyhow::Result<()> {
    let mut server = exec_server().await?;
    server.send_raw_text("not-json").await?;

    let response = server
        .wait_for_event(|event| matches!(event, JSONRPCMessage::Error(_)))
        .await?;
    let JSONRPCMessage::Error(JSONRPCError { id, error }) = response else {
        panic!("expected malformed-message error response");
    };
    assert_eq!(id, praxis_app_gateway_protocol::RequestId::Integer(-1));
    assert_eq!(error.code, -32600);
    assert!(
        error
            .message
            .starts_with("failed to parse websocket JSON-RPC message from exec-server websocket"),
        "unexpected malformed-message error: {}",
        error.message
    );

    let initialize_id = server
        .send_request(
            "initialize",
            serde_json::to_value(InitializeParams {
                client_name: "exec-server-test".to_string(),
            })?,
        )
        .await?;

    let response = server
        .wait_for_event(|event| {
            matches!(
                event,
                JSONRPCMessage::Response(JSONRPCResponse { id, .. }) if id == &initialize_id
            )
        })
        .await?;
    let JSONRPCMessage::Response(JSONRPCResponse { id, result }) = response else {
        panic!("expected initialize response after malformed input");
    };
    assert_eq!(id, initialize_id);
    let initialize_response: InitializeResponse = serde_json::from_value(result)?;
    assert_eq!(initialize_response, InitializeResponse {});

    server.shutdown().await?;
    Ok(())
}
