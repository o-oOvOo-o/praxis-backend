use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn includes_conversation_id_and_model_headers_in_request() {
    skip_if_no_network!();

    // Mock server
    let server = MockServer::start().await;

    let resp_mock = mount_sse_once(
        &server,
        sse(vec![ev_response_created("resp1"), ev_completed("resp1")]),
    )
    .await;

    let mut builder = test_praxis().with_auth(OpenAiAccountAuth::from_api_key("Test API Key"));
    let test = builder
        .build(&server)
        .await
        .expect("create new conversation");
    let praxis = test.thread.clone();
    let session_id = test.session_configured.session_id;

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

    let request = resp_mock.single_request();
    assert_eq!(request.path(), "/v1/responses");
    let request_session_id = request.header("session_id").expect("session_id header");
    let request_authorization = request
        .header("authorization")
        .expect("authorization header");
    let request_originator = request.header("originator").expect("originator header");

    assert_eq!(request_session_id, session_id.to_string());
    assert_eq!(request_originator, originator().value);
    assert_eq!(request_authorization, "Bearer Test API Key");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn provider_auth_command_supplies_bearer_token() {
    skip_if_no_network!();

    let server = MockServer::start().await;
    mount_sse_once_match(
        &server,
        header("authorization", "Bearer command-token"),
        sse(vec![ev_response_created("resp1"), ev_completed("resp1")]),
    )
    .await;
    let auth_fixture = ProviderAuthCommandFixture::new(&["command-token"]).unwrap();

    send_provider_auth_request(&server, auth_fixture.auth()).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn provider_auth_command_refreshes_after_401() {
    skip_if_no_network!();

    let server = MockServer::start().await;
    let auth_fixture = ProviderAuthCommandFixture::new(&["first-token", "second-token"]).unwrap();

    Mock::given(method("POST"))
        .and(path("/v1/responses"))
        .and(header_regex("Authorization", "Bearer first-token"))
        .respond_with(ResponseTemplate::new(401).set_body_string("unauthorized"))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/responses"))
        .and(header_regex("Authorization", "Bearer second-token"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_raw(
                    sse(vec![ev_response_created("resp1"), ev_completed("resp1")]),
                    "text/event-stream",
                ),
        )
        .expect(1)
        .mount(&server)
        .await;

    send_provider_auth_request(&server, auth_fixture.auth()).await;
}
