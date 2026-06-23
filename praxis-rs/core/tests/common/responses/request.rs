use super::*;

pub struct ResponseMock {
    requests: Arc<Mutex<Vec<ResponsesRequest>>>,
}

impl ResponseMock {
    pub(super) fn new() -> Self {
        Self {
            requests: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn single_request(&self) -> ResponsesRequest {
        let requests = self.requests.lock().unwrap();
        if requests.len() != 1 {
            panic!("expected 1 request, got {}", requests.len());
        }
        requests.first().unwrap().clone()
    }

    pub fn requests(&self) -> Vec<ResponsesRequest> {
        self.requests.lock().unwrap().clone()
    }

    pub fn last_request(&self) -> Option<ResponsesRequest> {
        self.requests.lock().unwrap().last().cloned()
    }

    /// Returns true if any captured request contains a `function_call` with the
    /// provided `call_id`.
    pub fn saw_function_call(&self, call_id: &str) -> bool {
        self.requests()
            .iter()
            .any(|req| req.has_function_call(call_id))
    }

    /// Returns the `output` string for a matching `function_call_output` with
    /// the provided `call_id`, searching across all captured requests.
    pub fn function_call_output_text(&self, call_id: &str) -> Option<String> {
        self.requests()
            .iter()
            .find_map(|req| req.function_call_output_text(call_id))
    }
}

#[derive(Debug, Clone)]
pub struct ResponsesRequest(wiremock::Request);

fn is_zstd_encoding(value: &str) -> bool {
    value
        .split(',')
        .any(|entry| entry.trim().eq_ignore_ascii_case("zstd"))
}

pub(super) fn decode_body_bytes(body: &[u8], content_encoding: Option<&str>) -> Vec<u8> {
    if content_encoding.is_some_and(is_zstd_encoding) {
        zstd::stream::decode_all(std::io::Cursor::new(body)).unwrap_or_else(|err| {
            panic!("failed to decode zstd request body: {err}");
        })
    } else {
        body.to_vec()
    }
}

impl ResponsesRequest {
    pub fn body_json(&self) -> Value {
        let body = decode_body_bytes(
            &self.0.body,
            self.0
                .headers
                .get("content-encoding")
                .and_then(|value| value.to_str().ok()),
        );
        serde_json::from_slice(&body).unwrap()
    }

    pub fn body_bytes(&self) -> Vec<u8> {
        self.0.body.clone()
    }

    pub fn body_contains_text(&self, text: &str) -> bool {
        let json_fragment = serde_json::to_string(text)
            .expect("serialize text to JSON")
            .trim_matches('"')
            .to_string();
        self.body_json().to_string().contains(&json_fragment)
    }

    pub fn instructions_text(&self) -> String {
        self.body_json()["instructions"]
            .as_str()
            .unwrap()
            .to_string()
    }

    /// Returns all `input_text` spans from `message` inputs for the provided role.
    pub fn message_input_texts(&self, role: &str) -> Vec<String> {
        self.inputs_of_type("message")
            .into_iter()
            .filter(|item| item.get("role").and_then(Value::as_str) == Some(role))
            .filter_map(|item| item.get("content").and_then(Value::as_array).cloned())
            .flatten()
            .filter(|span| span.get("type").and_then(Value::as_str) == Some("input_text"))
            .filter_map(|span| span.get("text").and_then(Value::as_str).map(str::to_owned))
            .collect()
    }

    /// Returns `input_text` spans grouped by `message` input for the provided role.
    pub fn message_input_text_groups(&self, role: &str) -> Vec<Vec<String>> {
        self.inputs_of_type("message")
            .into_iter()
            .filter(|item| item.get("role").and_then(Value::as_str) == Some(role))
            .filter_map(|item| item.get("content").and_then(Value::as_array).cloned())
            .map(|content| {
                content
                    .into_iter()
                    .filter(|span| span.get("type").and_then(Value::as_str) == Some("input_text"))
                    .filter_map(|span| span.get("text").and_then(Value::as_str).map(str::to_owned))
                    .collect()
            })
            .collect()
    }

    pub fn has_message_with_input_texts(
        &self,
        role: &str,
        predicate: impl Fn(&[String]) -> bool,
    ) -> bool {
        self.message_input_text_groups(role)
            .iter()
            .any(|texts| predicate(texts))
    }

    /// Returns all `input_image` `image_url` spans from `message` inputs for the provided role.
    pub fn message_input_image_urls(&self, role: &str) -> Vec<String> {
        self.inputs_of_type("message")
            .into_iter()
            .filter(|item| item.get("role").and_then(Value::as_str) == Some(role))
            .filter_map(|item| item.get("content").and_then(Value::as_array).cloned())
            .flatten()
            .filter(|span| span.get("type").and_then(Value::as_str) == Some("input_image"))
            .filter_map(|span| {
                span.get("image_url")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
            })
            .collect()
    }

    pub fn input(&self) -> Vec<Value> {
        self.body_json()["input"]
            .as_array()
            .expect("input array not found in request")
            .clone()
    }

    pub fn inputs_of_type(&self, ty: &str) -> Vec<Value> {
        self.input()
            .iter()
            .filter(|item| item.get("type").and_then(Value::as_str) == Some(ty))
            .cloned()
            .collect()
    }

    pub fn function_call_output(&self, call_id: &str) -> Value {
        self.call_output(call_id, "function_call_output")
    }

    pub fn custom_tool_call_output(&self, call_id: &str) -> Value {
        self.call_output(call_id, "custom_tool_call_output")
    }

    pub fn tool_search_output(&self, call_id: &str) -> Value {
        self.call_output(call_id, "tool_search_output")
    }

    pub fn call_output(&self, call_id: &str, call_type: &str) -> Value {
        self.input()
            .iter()
            .find(|item| {
                item.get("type").unwrap() == call_type && item.get("call_id").unwrap() == call_id
            })
            .cloned()
            .unwrap_or_else(|| panic!("function call output {call_id} item not found in request"))
    }

    /// Returns true if this request's `input` contains a `function_call` with
    /// the specified `call_id`.
    pub fn has_function_call(&self, call_id: &str) -> bool {
        self.input().iter().any(|item| {
            item.get("type").and_then(Value::as_str) == Some("function_call")
                && item.get("call_id").and_then(Value::as_str) == Some(call_id)
        })
    }

    /// If present, returns the `output` string of the `function_call_output`
    /// entry matching `call_id` in this request's `input`.
    pub fn function_call_output_text(&self, call_id: &str) -> Option<String> {
        let binding = self.input();
        let item = binding.iter().find(|item| {
            item.get("type").and_then(Value::as_str) == Some("function_call_output")
                && item.get("call_id").and_then(Value::as_str) == Some(call_id)
        })?;
        item.get("output")
            .and_then(Value::as_str)
            .map(str::to_string)
    }

    pub fn function_call_output_content_and_success(
        &self,
        call_id: &str,
    ) -> Option<(Option<String>, Option<bool>)> {
        self.call_output_content_and_success(call_id, "function_call_output")
    }

    pub fn custom_tool_call_output_content_and_success(
        &self,
        call_id: &str,
    ) -> Option<(Option<String>, Option<bool>)> {
        self.call_output_content_and_success(call_id, "custom_tool_call_output")
    }

    fn call_output_content_and_success(
        &self,
        call_id: &str,
        call_type: &str,
    ) -> Option<(Option<String>, Option<bool>)> {
        let output = self
            .call_output(call_id, call_type)
            .get("output")
            .cloned()
            .unwrap_or(Value::Null);
        match output {
            Value::String(_) | Value::Array(_) => Some((output_value_to_text(&output), None)),
            Value::Object(obj) => Some((
                obj.get("content")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                obj.get("success").and_then(Value::as_bool),
            )),
            _ => Some((None, None)),
        }
    }

    pub fn header(&self, name: &str) -> Option<String> {
        self.0
            .headers
            .get(name)
            .and_then(|v| v.to_str().ok())
            .map(str::to_string)
    }

    pub fn path(&self) -> String {
        self.0.url.path().to_string()
    }

    pub fn query_param(&self, name: &str) -> Option<String> {
        self.0
            .url
            .query_pairs()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.to_string())
    }
}

pub(crate) fn output_value_to_text(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.clone()),
        Value::Array(items) => match items.as_slice() {
            [item] if item.get("type").and_then(Value::as_str) == Some("input_text") => {
                item.get("text").and_then(Value::as_str).map(str::to_string)
            }
            [_] | [] | [_, _, ..] => None,
        },
        Value::Object(_) | Value::Number(_) | Value::Bool(_) | Value::Null => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use wiremock::http::HeaderMap;
    use wiremock::http::Method;

    fn request_with_input(input: Value) -> ResponsesRequest {
        ResponsesRequest(wiremock::Request {
            url: "http://localhost/v1/responses"
                .parse()
                .expect("valid request url"),
            method: Method::POST,
            headers: HeaderMap::new(),
            body: serde_json::to_vec(&serde_json::json!({ "input": input }))
                .expect("serialize request body"),
        })
    }

    #[test]
    fn call_output_content_and_success_returns_only_single_text_content_item() {
        let single_text = request_with_input(serde_json::json!([
            {
                "type": "function_call_output",
                "call_id": "call-1",
                "output": [{ "type": "input_text", "text": "hello" }]
            },
            {
                "type": "custom_tool_call_output",
                "call_id": "call-2",
                "output": [{ "type": "input_text", "text": "world" }]
            }
        ]));
        assert_eq!(
            single_text.function_call_output_content_and_success("call-1"),
            Some((Some("hello".to_string()), None))
        );
        assert_eq!(
            single_text.custom_tool_call_output_content_and_success("call-2"),
            Some((Some("world".to_string()), None))
        );

        let mixed_content = request_with_input(serde_json::json!([
            {
                "type": "function_call_output",
                "call_id": "call-3",
                "output": [
                    { "type": "input_text", "text": "hello" },
                    { "type": "input_image", "image_url": "data:image/png;base64,abc" }
                ]
            },
            {
                "type": "custom_tool_call_output",
                "call_id": "call-4",
                "output": [{ "type": "input_image", "image_url": "data:image/png;base64,abc" }]
            }
        ]));
        assert_eq!(
            mixed_content.function_call_output_content_and_success("call-3"),
            Some((None, None))
        );
        assert_eq!(
            mixed_content.custom_tool_call_output_content_and_success("call-4"),
            Some((None, None))
        );
    }
}

#[derive(Debug, Clone)]

impl Match for ResponseMock {
    fn matches(&self, request: &wiremock::Request) -> bool {
        self.requests
            .lock()
            .unwrap()
            .push(ResponsesRequest(request.clone()));

        // Enforce invariant checks on every request body captured by the mock.
        // Panic on orphan tool outputs or calls to catch regressions early.
        validate_request_body_invariants(request);
        true
    }
}

/// Build an SSE stream body from a list of JSON events.

/// Validate invariants on the request body sent to `/v1/responses`.
///
/// - No `function_call_output`/`custom_tool_call_output` with missing/empty `call_id`.
/// - `tool_search_output` must have a `call_id` unless it is a server-executed legacy item.
/// - Every `function_call_output` must match a prior `function_call` or
///   `local_shell_call` with the same `call_id` in the same `input`.
/// - Every `custom_tool_call_output` must match a prior `custom_tool_call`.
/// - Every `tool_search_output` must match a prior `tool_search_call`.
/// - Additionally, enforce symmetry: every `function_call`/`custom_tool_call`/
///   `tool_search_call` in the `input` must have a matching output entry.
fn validate_request_body_invariants(request: &wiremock::Request) {
    // Skip GET requests (e.g., /models)
    if request.method != "POST" || !request.url.path().ends_with("/responses") {
        return;
    }
    let body_bytes = decode_body_bytes(
        &request.body,
        request
            .headers
            .get("content-encoding")
            .and_then(|value| value.to_str().ok()),
    );
    let Ok(body): Result<Value, _> = serde_json::from_slice(&body_bytes) else {
        return;
    };
    let Some(items) = body.get("input").and_then(Value::as_array) else {
        panic!("input array not found in request");
    };

    use std::collections::HashSet;

    fn get_call_id(item: &Value) -> Option<&str> {
        item.get("call_id")
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty())
    }

    fn gather_ids(items: &[Value], kind: &str) -> HashSet<String> {
        items
            .iter()
            .filter(|item| item.get("type").and_then(Value::as_str) == Some(kind))
            .filter_map(get_call_id)
            .map(str::to_string)
            .collect()
    }

    fn gather_output_ids(items: &[Value], kind: &str, missing_msg: &str) -> HashSet<String> {
        items
            .iter()
            .filter(|item| item.get("type").and_then(Value::as_str) == Some(kind))
            .map(|item| {
                let Some(id) = get_call_id(item) else {
                    panic!("{missing_msg}");
                };
                id.to_string()
            })
            .collect()
    }

    fn gather_tool_search_output_ids(items: &[Value]) -> HashSet<String> {
        items
            .iter()
            .filter(|item| item.get("type").and_then(Value::as_str) == Some("tool_search_output"))
            .filter_map(|item| {
                if let Some(id) = get_call_id(item) {
                    return Some(id.to_string());
                }
                if item.get("execution").and_then(Value::as_str) == Some("server") {
                    return None;
                }
                panic!("orphan tool_search_output with empty call_id should be dropped");
            })
            .collect()
    }

    let function_calls = gather_ids(items, "function_call");
    let tool_search_calls = gather_ids(items, "tool_search_call");
    let custom_tool_calls = gather_ids(items, "custom_tool_call");
    let local_shell_calls = gather_ids(items, "local_shell_call");
    let function_call_outputs = gather_output_ids(
        items,
        "function_call_output",
        "orphan function_call_output with empty call_id should be dropped",
    );
    let tool_search_outputs = gather_tool_search_output_ids(items);
    let custom_tool_call_outputs = gather_output_ids(
        items,
        "custom_tool_call_output",
        "orphan custom_tool_call_output with empty call_id should be dropped",
    );

    for cid in &function_call_outputs {
        assert!(
            function_calls.contains(cid) || local_shell_calls.contains(cid),
            "function_call_output without matching call in input: {cid}",
        );
    }
    for cid in &custom_tool_call_outputs {
        assert!(
            custom_tool_calls.contains(cid),
            "custom_tool_call_output without matching call in input: {cid}",
        );
    }
    for cid in &tool_search_outputs {
        assert!(
            tool_search_calls.contains(cid),
            "tool_search_output without matching call in input: {cid}",
        );
    }

    for cid in &function_calls {
        assert!(
            function_call_outputs.contains(cid),
            "Function call output is missing for call id: {cid}",
        );
    }
    for cid in &custom_tool_calls {
        assert!(
            custom_tool_call_outputs.contains(cid),
            "Custom tool call output is missing for call id: {cid}",
        );
    }
    for cid in &tool_search_calls {
        assert!(
            tool_search_outputs.contains(cid),
            "Tool search output is missing for call id: {cid}",
        );
    }
}
