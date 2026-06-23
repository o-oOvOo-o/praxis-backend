use super::*;

pub fn sse_failed(id: &str, code: &str, message: &str) -> String {
    sse(vec![serde_json::json!({
        "type": "response.failed",
        "response": {
            "id": id,
            "error": {"code": code, "message": message}
        }
    })])
}

pub fn sse_response(body: String) -> ResponseTemplate {
    ResponseTemplate::new(200)
        .insert_header("content-type", "text/event-stream")
        .set_body_raw(body, "text/event-stream")
}

pub async fn mount_response_once(server: &MockServer, response: ResponseTemplate) -> ResponseMock {
    let (mock, response_mock) = base_mock();
    mock.respond_with(response)
        .up_to_n_times(1)
        .mount(server)
        .await;
    response_mock
}

pub async fn mount_response_once_match<M>(
    server: &MockServer,
    matcher: M,
    response: ResponseTemplate,
) -> ResponseMock
where
    M: wiremock::Match + Send + Sync + 'static,
{
    let (mock, response_mock) = base_mock();
    mock.and(matcher)
        .respond_with(response)
        .up_to_n_times(1)
        .mount(server)
        .await;
    response_mock
}

fn base_mock() -> (MockBuilder, ResponseMock) {
    let response_mock = ResponseMock::new();
    let mock = Mock::given(method("POST"))
        .and(path_regex(".*/responses$"))
        .and(response_mock.clone());
    (mock, response_mock)
}

fn compact_mock() -> (MockBuilder, ResponseMock) {
    let response_mock = ResponseMock::new();
    let mock = Mock::given(method("POST"))
        .and(path_regex(".*/responses/compact$"))
        .and(response_mock.clone());
    (mock, response_mock)
}

fn models_mock() -> (MockBuilder, ModelsMock) {
    let models_mock = ModelsMock::new();
    let mock = Mock::given(method("GET"))
        .and(path_regex(".*/models$"))
        .and(models_mock.clone());
    (mock, models_mock)
}

pub async fn mount_sse_once_match<M>(server: &MockServer, matcher: M, body: String) -> ResponseMock
where
    M: wiremock::Match + Send + Sync + 'static,
{
    let (mock, response_mock) = base_mock();
    mock.and(matcher)
        .respond_with(sse_response(body))
        .up_to_n_times(1)
        .mount(server)
        .await;
    response_mock
}

pub async fn mount_sse_once(server: &MockServer, body: String) -> ResponseMock {
    let (mock, response_mock) = base_mock();
    mock.respond_with(sse_response(body))
        .up_to_n_times(1)
        .mount(server)
        .await;
    response_mock
}

pub async fn mount_compact_json_once_match<M>(
    server: &MockServer,
    matcher: M,
    body: serde_json::Value,
) -> ResponseMock
where
    M: wiremock::Match + Send + Sync + 'static,
{
    let (mock, response_mock) = compact_mock();
    mock.and(matcher)
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "application/json")
                .set_body_json(body.clone()),
        )
        .up_to_n_times(1)
        .mount(server)
        .await;
    response_mock
}

pub async fn mount_compact_json_once(server: &MockServer, body: serde_json::Value) -> ResponseMock {
    mount_compact_response_once(
        server,
        ResponseTemplate::new(200)
            .insert_header("content-type", "application/json")
            .set_body_json(body),
    )
    .await
}

/// Mount a `/responses/compact` mock that mirrors the default remote compaction shape:
/// keep user+developer messages from the request, drop assistant/tool artifacts, and append one
/// compaction item carrying the provided summary text.
pub async fn mount_compact_user_history_with_summary_once(
    server: &MockServer,
    summary_text: &str,
) -> ResponseMock {
    mount_compact_user_history_with_summary_sequence(server, vec![summary_text.to_string()]).await
}

/// Same as [`mount_compact_user_history_with_summary_once`], but for multiple compact calls.
/// Each incoming compact request receives the next summary text in order.
pub async fn mount_compact_user_history_with_summary_sequence(
    server: &MockServer,
    summary_texts: Vec<String>,
) -> ResponseMock {
    use std::sync::atomic::AtomicUsize;
    use std::sync::atomic::Ordering;

    #[derive(Debug)]
    struct UserHistorySummaryResponder {
        num_calls: AtomicUsize,
        summary_texts: Vec<String>,
    }

    impl Respond for UserHistorySummaryResponder {
        fn respond(&self, request: &wiremock::Request) -> ResponseTemplate {
            let call_num = self.num_calls.fetch_add(1, Ordering::SeqCst);
            let Some(summary_text) = self.summary_texts.get(call_num) else {
                panic!("no summary text for compact request {call_num}");
            };
            let body_bytes = decode_body_bytes(
                &request.body,
                request
                    .headers
                    .get("content-encoding")
                    .and_then(|value| value.to_str().ok()),
            );
            let body_json: Value = serde_json::from_slice(&body_bytes)
                .unwrap_or_else(|err| panic!("failed to parse compact request body: {err}"));
            let mut output = body_json
                .get("input")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default()
                .into_iter()
                // TODO(ccunningham): Update this mock to match future compaction model behavior:
                // return user/developer/assistant messages since the last compaction item, then
                // append a single newest compaction item.
                // Match current remote compaction behavior: keep user/developer messages and
                // omit assistant/tool history entries.
                .filter(|item| {
                    item.get("type").and_then(Value::as_str) == Some("message")
                        && matches!(
                            item.get("role").and_then(Value::as_str),
                            Some("user") | Some("developer")
                        )
                })
                .collect::<Vec<Value>>();
            // Append a synthetic compaction item as the newest item.
            output.push(serde_json::json!({
                "type": "compaction",
                "encrypted_content": summary_text,
            }));
            ResponseTemplate::new(200)
                .insert_header("content-type", "application/json")
                .set_body_json(serde_json::json!({ "output": output }))
        }
    }

    let num_calls = summary_texts.len();
    let responder = UserHistorySummaryResponder {
        num_calls: AtomicUsize::new(0),
        summary_texts,
    };
    let (mock, response_mock) = compact_mock();
    mock.respond_with(responder)
        .up_to_n_times(num_calls as u64)
        .expect(num_calls as u64)
        .mount(server)
        .await;
    response_mock
}

pub async fn mount_compact_response_once(
    server: &MockServer,
    response: ResponseTemplate,
) -> ResponseMock {
    let (mock, response_mock) = compact_mock();
    mock.respond_with(response)
        .up_to_n_times(1)
        .mount(server)
        .await;
    response_mock
}

pub async fn mount_models_once(server: &MockServer, body: ModelsResponse) -> ModelsMock {
    let (mock, models_mock) = models_mock();
    mock.respond_with(
        ResponseTemplate::new(200)
            .insert_header("content-type", "application/json")
            .set_body_json(body.clone()),
    )
    .up_to_n_times(1)
    .mount(server)
    .await;
    models_mock
}

pub async fn mount_models_once_with_delay(
    server: &MockServer,
    body: ModelsResponse,
    delay: Duration,
) -> ModelsMock {
    let (mock, models_mock) = models_mock();
    mock.respond_with(
        ResponseTemplate::new(200)
            .insert_header("content-type", "application/json")
            .set_body_json(body.clone())
            .set_delay(delay),
    )
    .up_to_n_times(1)
    .mount(server)
    .await;
    models_mock
}

pub async fn mount_models_once_with_etag(
    server: &MockServer,
    body: ModelsResponse,
    etag: &str,
) -> ModelsMock {
    let (mock, models_mock) = models_mock();
    mock.respond_with(
        ResponseTemplate::new(200)
            .insert_header("content-type", "application/json")
            // ModelsClient reads the ETag header, not a JSON field.
            .insert_header("ETag", etag)
            .set_body_json(body.clone()),
    )
    .up_to_n_times(1)
    .mount(server)
    .await;
    models_mock
}

pub async fn start_mock_server() -> MockServer {
    let server = MockServer::builder()
        .body_print_limit(BodyPrintLimit::Limited(80_000))
        .start()
        .await;

    // Provide a default `/models` response so tests remain hermetic when the client queries it.
    let _ = mount_models_once(&server, ModelsResponse { models: Vec::new() }).await;

    server
}

/// Starts a lightweight WebSocket server for `/v1/responses` tests.
///
/// Each connection consumes a queue of request/event sequences. For each
/// request message, the server records the payload and streams the matching
/// events as WebSocket text frames before moving to the next request.

pub struct FunctionCallResponseMocks {
    pub function_call: ResponseMock,
    pub completion: ResponseMock,
}

pub async fn mount_function_call_agent_response(
    server: &MockServer,
    call_id: &str,
    arguments: &str,
    tool_name: &str,
) -> FunctionCallResponseMocks {
    let first_response = sse(vec![
        ev_response_created("resp-1"),
        ev_function_call(call_id, tool_name, arguments),
        ev_completed("resp-1"),
    ]);
    let function_call = mount_sse_once(server, first_response).await;

    let second_response = sse(vec![
        ev_assistant_message("msg-1", "done"),
        ev_completed("resp-2"),
    ]);
    let completion = mount_sse_once(server, second_response).await;

    FunctionCallResponseMocks {
        function_call,
        completion,
    }
}

/// Mounts a sequence of SSE response bodies and serves them in order for each
/// POST to `/v1/responses`. Panics if more requests are received than bodies
/// provided. Also asserts the exact number of expected calls.
pub async fn mount_sse_sequence(server: &MockServer, bodies: Vec<String>) -> ResponseMock {
    use std::sync::atomic::AtomicUsize;
    use std::sync::atomic::Ordering;

    struct SeqResponder {
        num_calls: AtomicUsize,
        responses: Vec<String>,
    }

    impl Respond for SeqResponder {
        fn respond(&self, _: &wiremock::Request) -> ResponseTemplate {
            let call_num = self.num_calls.fetch_add(1, Ordering::SeqCst);
            match self.responses.get(call_num) {
                Some(body) => ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_string(body.clone()),
                None => panic!("no response for {call_num}"),
            }
        }
    }

    let num_calls = bodies.len();
    let responder = SeqResponder {
        num_calls: AtomicUsize::new(0),
        responses: bodies,
    };

    let (mock, response_mock) = base_mock();
    mock.respond_with(responder)
        .up_to_n_times(num_calls as u64)
        .expect(num_calls as u64)
        .mount(server)
        .await;

    response_mock
}

/// Mounts a sequence of responses for each POST to `/v1/responses`.
/// Panics if more requests are received than responses provided.
pub async fn mount_response_sequence(
    server: &MockServer,
    responses: Vec<ResponseTemplate>,
) -> ResponseMock {
    use std::sync::atomic::AtomicUsize;
    use std::sync::atomic::Ordering;

    struct SeqResponder {
        num_calls: AtomicUsize,
        responses: Vec<ResponseTemplate>,
    }

    impl Respond for SeqResponder {
        fn respond(&self, _: &wiremock::Request) -> ResponseTemplate {
            let call_num = self.num_calls.fetch_add(1, Ordering::SeqCst);
            self.responses
                .get(call_num)
                .unwrap_or_else(|| panic!("no response for {call_num}"))
                .clone()
        }
    }

    let num_calls = responses.len();
    let responder = SeqResponder {
        num_calls: AtomicUsize::new(0),
        responses,
    };

    let (mock, response_mock) = base_mock();
    mock.respond_with(responder)
        .up_to_n_times(num_calls as u64)
        .expect(num_calls as u64)
        .mount(server)
        .await;
    response_mock
}
