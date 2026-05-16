#![cfg(unix)]

mod common;

use common::exec_server::exec_server;
use praxis_app_gateway_protocol::JSONRPCMessage;
use praxis_app_gateway_protocol::JSONRPCResponse;
use praxis_exec_server::InitializeParams;
use praxis_exec_server::InitializeResponse;
use pretty_assertions::assert_eq;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn exec_server_accepts_initialize() -> anyhow::Result<()> {
    let mut server = exec_server().await?;
    let initialize_id = server
        .send_request(
            "initialize",
            serde_json::to_value(InitializeParams {
                client_name: "exec-server-test".to_string(),
            })?,
        )
        .await?;

    let response = server.next_event().await?;
    let JSONRPCMessage::Response(JSONRPCResponse { id, result }) = response else {
        panic!("expected initialize response");
    };
    assert_eq!(id, initialize_id);
    let initialize_response: InitializeResponse = serde_json::from_value(result)?;
    assert_eq!(initialize_response, InitializeResponse {});

    server.shutdown().await?;
    Ok(())
}
