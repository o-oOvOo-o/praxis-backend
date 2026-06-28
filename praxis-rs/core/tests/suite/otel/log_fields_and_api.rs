use super::*;

#[test]
fn extract_log_field_handles_empty_bare_values() {
    let line = "event.name=\"praxis.tool_result\" mcp_server= mcp_server_origin=";
    assert_eq!(extract_log_field(line, "mcp_server"), Some(String::new()));
    assert_eq!(
        extract_log_field(line, "mcp_server_origin"),
        Some(String::new())
    );
}

#[test]
fn extract_log_field_does_not_confuse_similar_keys() {
    let line = "event.name=\"praxis.tool_result\" mcp_server_origin=stdio";
    assert_eq!(extract_log_field(line, "mcp_server"), None);
    assert_eq!(
        extract_log_field(line, "mcp_server_origin"),
        Some("stdio".to_string())
    );
}

#[tokio::test]
#[traced_test]
async fn responses_api_emits_api_request_event() {
    let server = start_mock_server().await;

    mount_sse_once(&server, sse(vec![ev_completed("done")])).await;

    let TestPraxis { thread: praxis, .. } = test_praxis().build(&server).await.unwrap();

    codex
        .submit_user_turn(
            vec![UserInput::Text {
                text: "hello".into(),
                text_elements: Vec::new(),
            }],
            None,
        )
        .await
        .unwrap();

    wait_for_event(&praxis, |ev| matches!(ev, EventMsg::TurnComplete(_))).await;

    logs_assert(|lines: &[&str]| {
        lines
            .iter()
            .find(|line| line.contains("praxis.api_request"))
            .map(|_| Ok(()))
            .unwrap_or_else(|| Err("expected praxis.api_request event".to_string()))
    });

    logs_assert(|lines: &[&str]| {
        lines
            .iter()
            .find(|line| line.contains("praxis.conversation_starts"))
            .map(|_| Ok(()))
            .unwrap_or_else(|| Err("expected praxis.conversation_starts event".to_string()))
    });
}

#[tokio::test]
#[traced_test]
async fn process_sse_emits_tracing_for_output_item() {
    let server = start_mock_server().await;

    mount_sse_once(
        &server,
        sse(vec![ev_assistant_message("id1", "hi"), ev_completed("id2")]),
    )
    .await;

    let TestPraxis { thread: praxis, .. } = test_praxis().build(&server).await.unwrap();

    codex
        .submit_user_turn(
            vec![UserInput::Text {
                text: "hello".into(),
                text_elements: Vec::new(),
            }],
            None,
        )
        .await
        .unwrap();

    wait_for_event(&praxis, |ev| matches!(ev, EventMsg::TurnComplete(_))).await;

    logs_assert(|lines: &[&str]| {
        lines
            .iter()
            .find(|line| {
                line.contains("praxis.sse_event")
                    && line.contains("event.kind=response.output_item.done")
            })
            .map(|_| Ok(()))
            .unwrap_or(Err("missing response.output_item.done event".to_string()))
    });
}
