use super::*;

pub struct WebSocketRequest {
    body: Value,
}

impl WebSocketRequest {
    pub fn body_json(&self) -> Value {
        self.body.clone()
    }
}

#[derive(Debug, Clone)]
pub struct WebSocketHandshake {
    uri: String,
    headers: Vec<(String, String)>,
}

impl WebSocketHandshake {
    pub fn uri(&self) -> &str {
        &self.uri
    }

    pub fn header(&self, name: &str) -> Option<String> {
        self.headers
            .iter()
            .find(|(header, _)| header.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.clone())
    }
}

#[derive(Debug, Clone)]
pub struct WebSocketConnectionConfig {
    pub requests: Vec<Vec<Value>>,
    pub response_headers: Vec<(String, String)>,
    /// Optional delay inserted before accepting the websocket handshake.
    ///
    /// Tests use this to force websocket setup into an in-flight state so first-turn warmup paths
    /// can be exercised deterministically.
    pub accept_delay: Option<Duration>,
    /// Whether the server should send a websocket close frame after all scripted responses.
    ///
    /// Tests can disable this to simulate a peer that surfaces a terminal event but never
    /// completes the close handshake.
    pub close_after_requests: bool,
}

pub struct WebSocketTestServer {
    uri: String,
    connections: Arc<Mutex<Vec<Vec<WebSocketRequest>>>>,
    handshakes: Arc<Mutex<Vec<WebSocketHandshake>>>,
    request_log_updated: Arc<Notify>,
    shutdown: oneshot::Sender<()>,
    task: tokio::task::JoinHandle<()>,
}

impl WebSocketTestServer {
    pub fn uri(&self) -> &str {
        &self.uri
    }

    pub fn connections(&self) -> Vec<Vec<WebSocketRequest>> {
        self.connections.lock().unwrap().clone()
    }

    pub fn single_connection(&self) -> Vec<WebSocketRequest> {
        let connections = self.connections.lock().unwrap();
        if connections.len() != 1 {
            panic!("expected 1 connection, got {}", connections.len());
        }
        connections.first().cloned().unwrap_or_default()
    }

    pub async fn wait_for_request(
        &self,
        connection_index: usize,
        request_index: usize,
    ) -> WebSocketRequest {
        loop {
            if let Some(request) = self
                .connections
                .lock()
                .unwrap()
                .get(connection_index)
                .and_then(|connection| connection.get(request_index))
                .cloned()
            {
                return request;
            }
            self.request_log_updated.notified().await;
        }
    }

    pub fn handshakes(&self) -> Vec<WebSocketHandshake> {
        self.handshakes.lock().unwrap().clone()
    }

    /// Waits until at least `expected` websocket handshakes have been observed or timeout elapses.
    ///
    /// Uses a short bounded polling interval so tests can deterministically wait for background
    /// websocket activity without busy-spinning.
    pub async fn wait_for_handshakes(&self, expected: usize, timeout: Duration) -> bool {
        if self.handshakes.lock().unwrap().len() >= expected {
            return true;
        }

        let deadline = tokio::time::Instant::now() + timeout;
        let poll_interval = Duration::from_millis(10);
        loop {
            if self.handshakes.lock().unwrap().len() >= expected {
                return true;
            }
            let now = tokio::time::Instant::now();
            if now >= deadline {
                return false;
            }
            let sleep_for = std::cmp::min(poll_interval, deadline.saturating_duration_since(now));
            tokio::time::sleep(sleep_for).await;
        }
    }
    pub fn single_handshake(&self) -> WebSocketHandshake {
        let handshakes = self.handshakes.lock().unwrap();
        if handshakes.len() != 1 {
            panic!("expected 1 handshake, got {}", handshakes.len());
        }
        handshakes.first().cloned().unwrap()
    }

    pub async fn shutdown(self) {
        let _ = self.shutdown.send(());
        let _ = self.task.await;
    }
}

#[derive(Debug, Clone)]

pub async fn start_websocket_server(connections: Vec<Vec<Vec<Value>>>) -> WebSocketTestServer {
    let connections = connections
        .into_iter()
        .map(|requests| WebSocketConnectionConfig {
            requests,
            response_headers: Vec::new(),
            accept_delay: None,
            close_after_requests: true,
        })
        .collect();
    start_websocket_server_with_headers(connections).await
}

pub async fn start_websocket_server_with_headers(
    connections: Vec<WebSocketConnectionConfig>,
) -> WebSocketTestServer {
    let start = std::time::Instant::now();
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind websocket server");
    let addr = listener.local_addr().expect("websocket server address");
    let uri = format!("ws://{addr}");
    let connections_log = Arc::new(Mutex::new(Vec::new()));
    let handshakes_log = Arc::new(Mutex::new(Vec::new()));
    let request_log_updated = Arc::new(Notify::new());
    let requests = Arc::clone(&connections_log);
    let handshakes = Arc::clone(&handshakes_log);
    let request_log = Arc::clone(&request_log_updated);
    let connections = Arc::new(Mutex::new(VecDeque::from(connections)));
    let (shutdown_tx, mut shutdown_rx) = oneshot::channel();

    let task = tokio::spawn(async move {
        loop {
            let accept_res = tokio::select! {
                _ = &mut shutdown_rx => return,
                accept_res = listener.accept() => accept_res,
            };
            let (stream, _) = match accept_res {
                Ok(value) => value,
                Err(_) => return,
            };
            let connection = {
                let mut pending = connections.lock().unwrap();
                pending.pop_front()
            };

            let Some(connection) = connection else {
                continue;
            };

            if let Some(delay) = connection.accept_delay {
                tokio::time::sleep(delay).await;
            }

            let response_headers = connection.response_headers.clone();
            let handshake_log = Arc::clone(&handshakes);
            let callback = move |req: &Request, mut response: Response| {
                let headers = req
                    .headers()
                    .iter()
                    .filter_map(|(name, value)| {
                        value
                            .to_str()
                            .ok()
                            .map(|value| (name.as_str().to_string(), value.to_string()))
                    })
                    .collect();
                handshake_log.lock().unwrap().push(WebSocketHandshake {
                    uri: req.uri().to_string(),
                    headers,
                });

                let headers_mut = response.headers_mut();
                for (name, value) in &response_headers {
                    if let (Ok(name), Ok(value)) = (
                        HeaderName::from_bytes(name.as_bytes()),
                        HeaderValue::from_str(value),
                    ) {
                        headers_mut.insert(name, value);
                    }
                }

                Ok(response)
            };

            let mut ws_stream = match accept_hdr_async_with_config(
                stream,
                callback,
                Some(websocket_accept_config()),
            )
            .await
            {
                Ok(ws) => ws,
                Err(_) => continue,
            };

            let connection_index = {
                let mut log = requests.lock().unwrap();
                log.push(Vec::new());
                log.len() - 1
            };
            let close_after_requests = connection.close_after_requests;
            for request_events in connection.requests {
                let Some(Ok(message)) = ws_stream.next().await else {
                    break;
                };
                if let Some(body) = parse_ws_request_body(message) {
                    let mut log = requests.lock().unwrap();
                    if let Some(connection_log) = log.get_mut(connection_index) {
                        connection_log.push(WebSocketRequest { body });
                        let request_index = connection_log.len() - 1;
                        let request = &connection_log[request_index];
                        let request_body = request.body_json();
                        eprintln!(
                            "[ws test server +{}ms] connection={} received request={} type={:?} role={:?} text={:?} data={:?}",
                            start.elapsed().as_millis(),
                            connection_index,
                            request_index,
                            request_body.get("type").and_then(Value::as_str),
                            request_body
                                .get("item")
                                .and_then(|item| item.get("role"))
                                .and_then(Value::as_str),
                            request_body
                                .get("item")
                                .and_then(|item| item.get("content"))
                                .and_then(Value::as_array)
                                .and_then(|content| content.first())
                                .and_then(|content| content.get("text"))
                                .and_then(Value::as_str),
                            request_body
                                .get("item")
                                .and_then(|item| item.get("content"))
                                .and_then(Value::as_array)
                                .and_then(|content| content.first())
                                .and_then(|content| content.get("data"))
                                .and_then(Value::as_str),
                        );
                    }
                    request_log.notify_waiters();
                }

                eprintln!(
                    "[ws test server +{}ms] connection={} sending batch_size={} event_types={:?} audio_data={:?}",
                    start.elapsed().as_millis(),
                    connection_index,
                    request_events.len(),
                    request_events
                        .iter()
                        .map(|event| event.get("type").and_then(Value::as_str))
                        .collect::<Vec<_>>(),
                    request_events
                        .iter()
                        .find_map(|event| event.get("delta").and_then(Value::as_str)),
                );
                for event in &request_events {
                    let Ok(payload) = serde_json::to_string(event) else {
                        continue;
                    };
                    if ws_stream.send(Message::Text(payload.into())).await.is_err() {
                        break;
                    }
                }
            }

            if close_after_requests {
                let _ = ws_stream.close(None).await;
            } else {
                let _ = shutdown_rx.await;
                return;
            }

            if connections.lock().unwrap().is_empty() {
                return;
            }
        }
    });

    WebSocketTestServer {
        uri,
        connections: connections_log,
        handshakes: handshakes_log,
        request_log_updated,
        shutdown: shutdown_tx,
        task,
    }
}

fn parse_ws_request_body(message: Message) -> Option<Value> {
    match message {
        Message::Text(text) => serde_json::from_str(&text).ok(),
        Message::Binary(bytes) => serde_json::from_slice(&bytes).ok(),
        _ => None,
    }
}

fn websocket_accept_config() -> WebSocketConfig {
    let mut extensions = ExtensionsConfig::default();
    extensions.permessage_deflate = Some(DeflateConfig::default());

    let mut config = WebSocketConfig::default();
    config.extensions = extensions;
    config
}
