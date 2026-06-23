use super::*;

pub(super) enum ClientTransport {
    Stdio {
        child: Child,
        stdin: Option<ChildStdin>,
        stdout: BufReader<ChildStdout>,
    },
    WebSocket {
        url: String,
        socket: Box<WebSocket<MaybeTlsStream<TcpStream>>>,
    },
}

pub(super) struct PraxisClient {
    transport: ClientTransport,
    pending_notifications: VecDeque<JSONRPCNotification>,
    pub(super) command_approval_behavior: CommandApprovalBehavior,
    pub(super) command_approval_count: usize,
    pub(super) command_approval_item_ids: Vec<String>,
    pub(super) command_execution_statuses: Vec<CommandExecutionStatus>,
    pub(super) command_execution_outputs: Vec<String>,
    command_output_stream: String,
    command_item_started: bool,
    pub(super) helper_done_seen: bool,
    pub(super) turn_completed_before_helper_done: bool,
    pub(super) unexpected_items_before_helper_done: Vec<ThreadItem>,
    pub(super) last_turn_status: Option<TurnStatus>,
    pub(super) last_turn_error_message: Option<String>,
}

#[derive(Debug, Clone, Copy)]
pub(super) enum CommandApprovalBehavior {
    AlwaysAccept,
    AbortOn(usize),
}

pub(super) fn item_started_before_helper_done_is_unexpected(
    item: &ThreadItem,
    command_item_started: bool,
    helper_done_seen: bool,
) -> bool {
    if !command_item_started || helper_done_seen {
        return false;
    }

    !matches!(item, ThreadItem::UserMessage { .. })
}

impl PraxisClient {
    pub(super) fn connect(endpoint: &Endpoint, config_overrides: &[String]) -> Result<Self> {
        match endpoint {
            Endpoint::SpawnPraxis(praxis_bin) => Self::spawn_stdio(praxis_bin, config_overrides),
            Endpoint::ConnectWs(url) => Self::connect_websocket(url),
        }
    }

    pub(super) fn spawn_stdio(praxis_bin: &Path, config_overrides: &[String]) -> Result<Self> {
        let praxis_bin_display = praxis_bin.display();
        let mut cmd = Command::new(praxis_bin);
        if let Some(praxis_bin_parent) = praxis_bin.parent() {
            let mut path = OsString::from(praxis_bin_parent.as_os_str());
            if let Some(existing_path) = std::env::var_os("PATH") {
                path.push(":");
                path.push(existing_path);
            }
            cmd.env("PATH", path);
        }
        for override_kv in config_overrides {
            cmd.arg("--config").arg(override_kv);
        }
        let mut praxis_app_gateway = cmd
            .arg("app-gateway")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .with_context(|| format!("failed to start `{praxis_bin_display}` app-gateway"))?;

        let stdin = praxis_app_gateway
            .stdin
            .take()
            .context("praxis app-gateway stdin unavailable")?;
        let stdout = praxis_app_gateway
            .stdout
            .take()
            .context("praxis app-gateway stdout unavailable")?;

        Ok(Self {
            transport: ClientTransport::Stdio {
                child: praxis_app_gateway,
                stdin: Some(stdin),
                stdout: BufReader::new(stdout),
            },
            pending_notifications: VecDeque::new(),
            command_approval_behavior: CommandApprovalBehavior::AlwaysAccept,
            command_approval_count: 0,
            command_approval_item_ids: Vec::new(),
            command_execution_statuses: Vec::new(),
            command_execution_outputs: Vec::new(),
            command_output_stream: String::new(),
            command_item_started: false,
            helper_done_seen: false,
            turn_completed_before_helper_done: false,
            unexpected_items_before_helper_done: Vec::new(),
            last_turn_status: None,
            last_turn_error_message: None,
        })
    }

    pub(super) fn connect_websocket(url: &str) -> Result<Self> {
        let parsed = Url::parse(url).with_context(|| format!("invalid websocket URL `{url}`"))?;
        let deadline = Instant::now() + Duration::from_secs(10);
        let (socket, _response) = loop {
            match connect(parsed.as_str()) {
                Ok(result) => break result,
                Err(err) => {
                    if Instant::now() >= deadline {
                        return Err(err).with_context(|| {
                            format!(
                                "failed to connect to websocket app-gateway at `{url}`; if no server is running, start one with `praxis-app-gateway-test-client serve --listen {url}`"
                            )
                        });
                    }
                    thread::sleep(Duration::from_millis(50));
                }
            }
        };
        Ok(Self {
            transport: ClientTransport::WebSocket {
                url: url.to_string(),
                socket: Box::new(socket),
            },
            pending_notifications: VecDeque::new(),
            command_approval_behavior: CommandApprovalBehavior::AlwaysAccept,
            command_approval_count: 0,
            command_approval_item_ids: Vec::new(),
            command_execution_statuses: Vec::new(),
            command_execution_outputs: Vec::new(),
            command_output_stream: String::new(),
            command_item_started: false,
            helper_done_seen: false,
            turn_completed_before_helper_done: false,
            unexpected_items_before_helper_done: Vec::new(),
            last_turn_status: None,
            last_turn_error_message: None,
        })
    }

    pub(super) fn note_helper_output(&mut self, output: &str) {
        self.command_output_stream.push_str(output);
        if self
            .command_output_stream
            .contains("[elicitation-hold] done")
        {
            self.helper_done_seen = true;
        }
    }

    pub(super) fn initialize(&mut self) -> Result<InitializeResponse> {
        self.initialize_with_experimental_api(/*experimental_api*/ true)
    }

    pub(super) fn initialize_with_experimental_api(
        &mut self,
        experimental_api: bool,
    ) -> Result<InitializeResponse> {
        let request_id = self.request_id();
        let request = ClientRequest::Initialize {
            request_id: request_id.clone(),
            params: InitializeParams {
                client_info: ClientInfo {
                    name: "praxis-toy-app-gateway".to_string(),
                    title: Some("Praxis Toy App Gateway".to_string()),
                    version: env!("CARGO_PKG_VERSION").to_string(),
                },
                capabilities: Some(InitializeCapabilities {
                    experimental_api,
                    opt_out_notification_methods: Some(
                        NOTIFICATIONS_TO_OPT_OUT
                            .iter()
                            .map(|method| (*method).to_string())
                            .collect(),
                    ),
                }),
                host_extensions: Vec::new(),
            },
        };

        let response: InitializeResponse = self.send_request(request, request_id, "initialize")?;

        // Complete the initialize handshake.
        let initialized = JSONRPCMessage::Notification(JSONRPCNotification {
            method: "initialized".to_string(),
            params: None,
        });
        self.write_jsonrpc_message(initialized)?;

        Ok(response)
    }

    pub(super) fn thread_start(
        &mut self,
        params: ThreadStartParams,
    ) -> Result<ThreadStartResponse> {
        let request_id = self.request_id();
        let request = ClientRequest::ThreadStart {
            request_id: request_id.clone(),
            params,
        };

        self.send_request(request, request_id, "thread/start")
    }

    pub(super) fn thread_resume(
        &mut self,
        params: ThreadResumeParams,
    ) -> Result<ThreadResumeResponse> {
        let request_id = self.request_id();
        let request = ClientRequest::ThreadResume {
            request_id: request_id.clone(),
            params,
        };

        self.send_request(request, request_id, "thread/resume")
    }

    pub(super) fn turn_start(&mut self, params: TurnStartParams) -> Result<TurnStartResponse> {
        let request_id = self.request_id();
        let request = ClientRequest::TurnStart {
            request_id: request_id.clone(),
            params,
        };

        self.send_request(request, request_id, "turn/start")
    }

    pub(super) fn thread_control_acquire(
        &mut self,
        params: ThreadControlAcquireParams,
    ) -> Result<ThreadControlAcquireResponse> {
        let request_id = self.request_id();
        let request = ClientRequest::ThreadControlAcquire {
            request_id: request_id.clone(),
            params,
        };

        self.send_request(request, request_id, "thread/control/acquire")
    }

    pub(super) fn thread_control_release(
        &mut self,
        params: ThreadControlReleaseParams,
    ) -> Result<ThreadControlReleaseResponse> {
        let request_id = self.request_id();
        let request = ClientRequest::ThreadControlRelease {
            request_id: request_id.clone(),
            params,
        };

        self.send_request(request, request_id, "thread/control/release")
    }

    pub(super) fn login_account_chatgpt(&mut self) -> Result<LoginAccountResponse> {
        let request_id = self.request_id();
        let request = ClientRequest::LoginAccount {
            request_id: request_id.clone(),
            params: praxis_app_gateway_protocol::LoginAccountParams::Chatgpt,
        };

        self.send_request(request, request_id, "account/login/start")
    }

    pub(super) fn login_account_chatgpt_device_code(&mut self) -> Result<LoginAccountResponse> {
        let request_id = self.request_id();
        let request = ClientRequest::LoginAccount {
            request_id: request_id.clone(),
            params: praxis_app_gateway_protocol::LoginAccountParams::ChatgptDeviceCode,
        };

        self.send_request(request, request_id, "account/login/start")
    }

    pub(super) fn get_account_rate_limits(&mut self) -> Result<GetAccountRateLimitsResponse> {
        let request_id = self.request_id();
        let request = ClientRequest::GetAccountRateLimits {
            request_id: request_id.clone(),
            params: None,
        };

        self.send_request(request, request_id, "account/rateLimits/read")
    }

    pub(super) fn model_list(&mut self, params: ModelListParams) -> Result<ModelListResponse> {
        let request_id = self.request_id();
        let request = ClientRequest::ModelList {
            request_id: request_id.clone(),
            params,
        };

        self.send_request(request, request_id, "model/list")
    }

    pub(super) fn thread_list(&mut self, params: ThreadListParams) -> Result<ThreadListResponse> {
        let request_id = self.request_id();
        let request = ClientRequest::ThreadList {
            request_id: request_id.clone(),
            params,
        };

        self.send_request(request, request_id, "thread/list")
    }

    pub(super) fn thread_increment_elicitation(
        &mut self,
        params: ThreadIncrementElicitationParams,
    ) -> Result<ThreadIncrementElicitationResponse> {
        let request_id = self.request_id();
        let request = ClientRequest::ThreadIncrementElicitation {
            request_id: request_id.clone(),
            params,
        };

        self.send_request(request, request_id, "thread/increment_elicitation")
    }

    pub(super) fn thread_decrement_elicitation(
        &mut self,
        params: ThreadDecrementElicitationParams,
    ) -> Result<ThreadDecrementElicitationResponse> {
        let request_id = self.request_id();
        let request = ClientRequest::ThreadDecrementElicitation {
            request_id: request_id.clone(),
            params,
        };

        self.send_request(request, request_id, "thread/decrement_elicitation")
    }

    pub(super) fn wait_for_account_login_completion(
        &mut self,
        expected_login_id: &str,
    ) -> Result<AccountLoginCompletedNotification> {
        loop {
            let notification = self.next_notification()?;

            if let Ok(server_notification) = ServerNotification::try_from(notification) {
                match server_notification {
                    ServerNotification::AccountLoginCompleted(completion) => {
                        if completion.login_id.as_deref() == Some(expected_login_id) {
                            return Ok(completion);
                        }

                        println!(
                            "[ignoring account/login/completed for unexpected login_id: {:?}]",
                            completion.login_id
                        );
                    }
                    ServerNotification::AccountRateLimitsUpdated(snapshot) => {
                        println!("< accountRateLimitsUpdated notification: {snapshot:?}");
                    }
                    _ => {}
                }
            }
        }
    }

    pub(super) fn stream_turn(&mut self, thread_id: &str, turn_id: &str) -> Result<()> {
        loop {
            let notification = self.next_notification()?;

            let Ok(server_notification) = ServerNotification::try_from(notification) else {
                continue;
            };

            match server_notification {
                ServerNotification::ThreadStarted(payload) => {
                    if payload.thread.id == thread_id {
                        println!("< thread/started notification: {:?}", payload.thread);
                    }
                }
                ServerNotification::TurnStarted(payload) => {
                    if payload.turn.id == turn_id {
                        println!("< turn/started notification: {:?}", payload.turn.status);
                    }
                }
                ServerNotification::AgentMessageDelta(delta) => {
                    print!("{}", delta.delta);
                    std::io::stdout().flush().ok();
                }
                ServerNotification::CommandExecutionOutputDelta(delta) => {
                    self.note_helper_output(&delta.delta);
                    print!("{}", delta.delta);
                    std::io::stdout().flush().ok();
                }
                ServerNotification::TerminalInteraction(delta) => {
                    println!("[stdin sent: {}]", delta.stdin);
                    std::io::stdout().flush().ok();
                }
                ServerNotification::ItemStarted(payload) => {
                    if matches!(payload.item, ThreadItem::CommandExecution { .. }) {
                        if self.command_item_started && !self.helper_done_seen {
                            self.unexpected_items_before_helper_done
                                .push(payload.item.clone());
                        }
                        self.command_item_started = true;
                    } else if item_started_before_helper_done_is_unexpected(
                        &payload.item,
                        self.command_item_started,
                        self.helper_done_seen,
                    ) {
                        self.unexpected_items_before_helper_done
                            .push(payload.item.clone());
                    }
                    println!("\n< item started: {:?}", payload.item);
                }
                ServerNotification::ItemCompleted(payload) => {
                    if let ThreadItem::CommandExecution {
                        status,
                        aggregated_output,
                        ..
                    } = payload.item.clone()
                    {
                        self.command_execution_statuses.push(status);
                        if let Some(aggregated_output) = aggregated_output {
                            self.note_helper_output(&aggregated_output);
                            self.command_execution_outputs.push(aggregated_output);
                        }
                    }
                    println!("< item completed: {:?}", payload.item);
                }
                ServerNotification::TurnCompleted(payload) => {
                    if payload.turn.id == turn_id {
                        self.last_turn_status = Some(payload.turn.status.clone());
                        if self.command_item_started && !self.helper_done_seen {
                            self.turn_completed_before_helper_done = true;
                        }
                        self.last_turn_error_message = payload
                            .turn
                            .error
                            .as_ref()
                            .map(|error| error.message.clone());
                        println!("\n< turn/completed notification: {:?}", payload.turn.status);
                        if payload.turn.status == TurnStatus::Failed
                            && let Some(error) = payload.turn.error
                        {
                            println!("[turn error] {}", error.message);
                        }
                        break;
                    }
                }
                ServerNotification::McpToolCallProgress(payload) => {
                    println!("< MCP tool progress: {}", payload.message);
                }
                _ => {
                    println!("[UNKNOWN SERVER NOTIFICATION] {server_notification:?}");
                }
            }
        }

        Ok(())
    }

    pub(super) fn stream_notifications_forever(&mut self) -> Result<()> {
        loop {
            let _ = self.next_notification()?;
        }
    }

    pub(super) fn send_request<T>(
        &mut self,
        request: ClientRequest,
        request_id: RequestId,
        method: &str,
    ) -> Result<T>
    where
        T: DeserializeOwned,
    {
        let request_span = info_span!(
            "app_gateway_test_client.request",
            otel.kind = "client",
            otel.name = method,
            rpc.system = "jsonrpc",
            rpc.method = method,
            rpc.request_id = ?request_id,
        );
        request_span.in_scope(|| {
            self.write_request(&request)?;
            self.wait_for_response(request_id, method)
        })
    }

    pub(super) fn write_request(&mut self, request: &ClientRequest) -> Result<()> {
        let request_value = serde_json::to_value(request)?;
        let mut request: JSONRPCRequest = serde_json::from_value(request_value)
            .context("client request was not a valid JSON-RPC request")?;
        request.trace = current_span_w3c_trace_context();
        let request_json = serde_json::to_string(&request)?;
        let request_pretty = serde_json::to_string_pretty(&request)?;
        print_multiline_with_prefix("> ", &request_pretty);
        self.write_payload(&request_json)
    }

    pub(super) fn wait_for_response<T>(&mut self, request_id: RequestId, method: &str) -> Result<T>
    where
        T: DeserializeOwned,
    {
        loop {
            let message = self.read_jsonrpc_message()?;

            match message {
                JSONRPCMessage::Response(JSONRPCResponse { id, result }) => {
                    if id == request_id {
                        return serde_json::from_value(result)
                            .with_context(|| format!("{method} response missing payload"));
                    }
                }
                JSONRPCMessage::Error(err) => {
                    if err.id == request_id {
                        bail!("{method} failed: {err:?}");
                    }
                }
                JSONRPCMessage::Notification(notification) => {
                    self.pending_notifications.push_back(notification);
                }
                JSONRPCMessage::Request(request) => {
                    self.handle_server_request(request)?;
                }
            }
        }
    }

    pub(super) fn next_notification(&mut self) -> Result<JSONRPCNotification> {
        if let Some(notification) = self.pending_notifications.pop_front() {
            return Ok(notification);
        }

        loop {
            let message = self.read_jsonrpc_message()?;

            match message {
                JSONRPCMessage::Notification(notification) => return Ok(notification),
                JSONRPCMessage::Response(_) | JSONRPCMessage::Error(_) => {
                    // No outstanding requests, so ignore stray responses/errors for now.
                    continue;
                }
                JSONRPCMessage::Request(request) => {
                    self.handle_server_request(request)?;
                }
            }
        }
    }

    pub(super) fn read_jsonrpc_message(&mut self) -> Result<JSONRPCMessage> {
        loop {
            let raw = self.read_payload()?;
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                continue;
            }

            let parsed: Value =
                serde_json::from_str(trimmed).context("response was not valid JSON-RPC")?;
            let pretty = serde_json::to_string_pretty(&parsed)?;
            print_multiline_with_prefix("< ", &pretty);
            let message: JSONRPCMessage = serde_json::from_value(parsed)
                .context("response was not a valid JSON-RPC message")?;
            return Ok(message);
        }
    }

    pub(super) fn request_id(&self) -> RequestId {
        RequestId::String(Uuid::new_v4().to_string())
    }

    pub(super) fn handle_server_request(&mut self, request: JSONRPCRequest) -> Result<()> {
        let server_request = ServerRequest::try_from(request)
            .context("failed to deserialize ServerRequest from JSONRPCRequest")?;

        match server_request {
            ServerRequest::CommandExecutionRequestApproval { request_id, params } => {
                self.handle_command_execution_request_approval(request_id, params)?;
            }
            ServerRequest::FileChangeRequestApproval { request_id, params } => {
                self.approve_file_change_request(request_id, params)?;
            }
            other => {
                bail!("received unsupported server request: {other:?}");
            }
        }

        Ok(())
    }

    pub(super) fn handle_command_execution_request_approval(
        &mut self,
        request_id: RequestId,
        params: CommandExecutionRequestApprovalParams,
    ) -> Result<()> {
        let CommandExecutionRequestApprovalParams {
            thread_id,
            turn_id,
            item_id,
            approval_id,
            reason,
            network_approval_context,
            command,
            cwd,
            command_actions,
            additional_permissions,
            proposed_execpolicy_amendment,
            proposed_network_policy_amendments,
            available_decisions,
        } = params;

        println!(
            "\n< commandExecution approval requested for thread {thread_id}, turn {turn_id}, item {item_id}, approval {}",
            approval_id.as_deref().unwrap_or("<none>")
        );
        self.command_approval_count += 1;
        self.command_approval_item_ids.push(item_id.clone());
        if let Some(reason) = reason.as_deref() {
            println!("< reason: {reason}");
        }
        if let Some(network_approval_context) = network_approval_context.as_ref() {
            println!("< network approval context: {network_approval_context:?}");
        }
        if let Some(available_decisions) = available_decisions.as_ref() {
            println!("< available decisions: {available_decisions:?}");
        }
        if let Some(command) = command.as_deref() {
            println!("< command: {command}");
        }
        if let Some(cwd) = cwd.as_ref() {
            println!("< cwd: {}", cwd.display());
        }
        if let Some(command_actions) = command_actions.as_ref()
            && !command_actions.is_empty()
        {
            println!("< command actions: {command_actions:?}");
        }
        if let Some(additional_permissions) = additional_permissions.as_ref() {
            println!("< additional permissions: {additional_permissions:?}");
        }
        if let Some(execpolicy_amendment) = proposed_execpolicy_amendment.as_ref() {
            println!("< proposed execpolicy amendment: {execpolicy_amendment:?}");
        }
        if let Some(network_policy_amendments) = proposed_network_policy_amendments.as_ref() {
            println!("< proposed network policy amendments: {network_policy_amendments:?}");
        }

        let decision = match self.command_approval_behavior {
            CommandApprovalBehavior::AlwaysAccept => CommandExecutionApprovalDecision::Accept,
            CommandApprovalBehavior::AbortOn(index) if self.command_approval_count == index => {
                CommandExecutionApprovalDecision::Cancel
            }
            CommandApprovalBehavior::AbortOn(_) => CommandExecutionApprovalDecision::Accept,
        };
        let response = CommandExecutionRequestApprovalResponse {
            decision: decision.clone(),
        };
        self.send_server_request_response(request_id, &response)?;
        println!(
            "< commandExecution decision for approval #{} on item {item_id}: {:?}",
            self.command_approval_count, decision
        );
        Ok(())
    }

    pub(super) fn approve_file_change_request(
        &mut self,
        request_id: RequestId,
        params: FileChangeRequestApprovalParams,
    ) -> Result<()> {
        let FileChangeRequestApprovalParams {
            thread_id,
            turn_id,
            item_id,
            reason,
            grant_root,
        } = params;

        println!(
            "\n< fileChange approval requested for thread {thread_id}, turn {turn_id}, item {item_id}"
        );
        if let Some(reason) = reason.as_deref() {
            println!("< reason: {reason}");
        }
        if let Some(grant_root) = grant_root.as_deref() {
            println!("< grant root: {}", grant_root.display());
        }

        let response = FileChangeRequestApprovalResponse {
            decision: FileChangeApprovalDecision::Accept,
        };
        self.send_server_request_response(request_id, &response)?;
        println!("< approved fileChange request for item {item_id}");
        Ok(())
    }

    pub(super) fn send_server_request_response<T>(
        &mut self,
        request_id: RequestId,
        response: &T,
    ) -> Result<()>
    where
        T: Serialize,
    {
        let message = JSONRPCMessage::Response(JSONRPCResponse {
            id: request_id,
            result: serde_json::to_value(response)?,
        });
        self.write_jsonrpc_message(message)
    }

    pub(super) fn write_jsonrpc_message(&mut self, message: JSONRPCMessage) -> Result<()> {
        let payload = serde_json::to_string(&message)?;
        let pretty = serde_json::to_string_pretty(&message)?;
        print_multiline_with_prefix("> ", &pretty);
        self.write_payload(&payload)
    }

    pub(super) fn write_payload(&mut self, payload: &str) -> Result<()> {
        match &mut self.transport {
            ClientTransport::Stdio { stdin, .. } => {
                if let Some(stdin) = stdin.as_mut() {
                    writeln!(stdin, "{payload}")?;
                    stdin
                        .flush()
                        .context("failed to flush payload to praxis app-gateway")?;
                    return Ok(());
                }
                bail!("praxis app-gateway stdin closed")
            }
            ClientTransport::WebSocket { socket, url } => {
                socket
                    .send(Message::Text(payload.to_string().into()))
                    .with_context(|| format!("failed to write websocket message to `{url}`"))?;
                Ok(())
            }
        }
    }

    pub(super) fn read_payload(&mut self) -> Result<String> {
        match &mut self.transport {
            ClientTransport::Stdio { stdout, .. } => {
                let mut response_line = String::new();
                let bytes = stdout
                    .read_line(&mut response_line)
                    .context("failed to read from praxis app-gateway")?;
                if bytes == 0 {
                    bail!("praxis app-gateway closed stdout");
                }
                Ok(response_line)
            }
            ClientTransport::WebSocket { socket, url } => loop {
                let frame = socket
                    .read()
                    .with_context(|| format!("failed to read websocket message from `{url}`"))?;
                match frame {
                    Message::Text(text) => return Ok(text.to_string()),
                    Message::Binary(_) | Message::Ping(_) | Message::Pong(_) => continue,
                    Message::Close(_) => {
                        bail!("websocket app-gateway at `{url}` closed the connection")
                    }
                    Message::Frame(_) => continue,
                }
            },
        }
    }
}

pub(super) fn print_multiline_with_prefix(prefix: &str, payload: &str) {
    for line in payload.lines() {
        println!("{prefix}{line}");
    }
}

pub(super) struct TestClientTracing {
    _otel_provider: Option<OtelProvider>,
    pub(super) traces_enabled: bool,
}

impl TestClientTracing {
    pub(super) async fn initialize(config_overrides: &[String]) -> Result<Self> {
        let cli_kv_overrides = CliConfigOverrides {
            raw_overrides: config_overrides.to_vec(),
        }
        .parse_overrides()
        .map_err(|e| anyhow::anyhow!("error parsing -c overrides: {e}"))?;
        let config = Config::load_with_cli_overrides(cli_kv_overrides)
            .await
            .context("error loading config")?;
        let otel_provider = praxis_core::otel_init::build_provider(
            &config,
            env!("CARGO_PKG_VERSION"),
            Some(OTEL_SERVICE_NAME),
            DEFAULT_ANALYTICS_ENABLED,
        )
        .map_err(|e| anyhow::anyhow!("error loading otel config: {e}"))?;
        let traces_enabled = otel_provider
            .as_ref()
            .and_then(|provider| provider.tracer_provider.as_ref())
            .is_some();
        if let Some(provider) = otel_provider.as_ref()
            && traces_enabled
        {
            let _ = tracing_subscriber::registry()
                .with(provider.tracing_layer())
                .try_init();
        }
        Ok(Self {
            traces_enabled,
            _otel_provider: otel_provider,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum TraceSummary {
    Enabled { url: String },
    Disabled,
}

impl TraceSummary {
    pub(super) fn capture(traces_enabled: bool) -> Self {
        if !traces_enabled {
            return Self::Disabled;
        }
        current_span_w3c_trace_context()
            .as_ref()
            .and_then(trace_url_from_context)
            .map_or(Self::Disabled, |url| Self::Enabled { url })
    }
}

pub(super) fn trace_url_from_context(trace: &W3cTraceContext) -> Option<String> {
    let traceparent = trace.traceparent.as_deref()?;
    let mut parts = traceparent.split('-');
    match (parts.next(), parts.next(), parts.next(), parts.next()) {
        (Some(_version), Some(trace_id), Some(_span_id), Some(_trace_flags))
            if trace_id.len() == 32 =>
        {
            Some(format!("go/trace/{trace_id}"))
        }
        _ => None,
    }
}

pub(super) fn print_trace_summary(trace_summary: &TraceSummary) {
    println!("\n[Datadog trace]");
    match trace_summary {
        TraceSummary::Enabled { url } => println!("{url}\n"),
        TraceSummary::Disabled => println!("{TRACE_DISABLED_MESSAGE}\n"),
    }
}

impl Drop for PraxisClient {
    fn drop(&mut self) {
        let ClientTransport::Stdio { child, stdin, .. } = &mut self.transport else {
            return;
        };

        let _ = stdin.take();

        if let Ok(Some(status)) = child.try_wait() {
            println!("[praxis app-gateway exited: {status}]");
            return;
        }

        let deadline = SystemTime::now() + APP_GATEWAY_GRACEFUL_SHUTDOWN_TIMEOUT;
        loop {
            if let Ok(Some(status)) = child.try_wait() {
                println!("[praxis app-gateway exited: {status}]");
                return;
            }

            if SystemTime::now() >= deadline {
                break;
            }

            thread::sleep(APP_GATEWAY_GRACEFUL_SHUTDOWN_POLL_INTERVAL);
        }

        let _ = child.kill();
        let _ = child.wait();
    }
}
