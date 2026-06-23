use std::collections::VecDeque;
use std::path::Path;
use std::process::Stdio;
use std::sync::atomic::AtomicI64;
use std::sync::atomic::Ordering;
use tokio::io::AsyncBufReadExt;
use tokio::io::AsyncWriteExt;
use tokio::io::BufReader;
use tokio::process::Child;
use tokio::process::ChildStdin;
use tokio::process::ChildStdout;

use anyhow::Context;
use praxis_app_gateway_protocol::AppsListParams;
use praxis_app_gateway_protocol::CancelLoginAccountParams;
use praxis_app_gateway_protocol::ClientInfo;
use praxis_app_gateway_protocol::ClientNotification;
use praxis_app_gateway_protocol::CollaborationModeListParams;
use praxis_app_gateway_protocol::CommandExecParams;
use praxis_app_gateway_protocol::CommandExecResizeParams;
use praxis_app_gateway_protocol::CommandExecTerminateParams;
use praxis_app_gateway_protocol::CommandExecWriteParams;
use praxis_app_gateway_protocol::ConfigBatchWriteParams;
use praxis_app_gateway_protocol::ConfigReadParams;
use praxis_app_gateway_protocol::ConfigValueWriteParams;
use praxis_app_gateway_protocol::ExperimentalFeatureListParams;
use praxis_app_gateway_protocol::FeedbackUploadParams;
use praxis_app_gateway_protocol::FsCopyParams;
use praxis_app_gateway_protocol::FsCreateDirectoryParams;
use praxis_app_gateway_protocol::FsGetMetadataParams;
use praxis_app_gateway_protocol::FsReadDirectoryParams;
use praxis_app_gateway_protocol::FsReadFileParams;
use praxis_app_gateway_protocol::FsRemoveParams;
use praxis_app_gateway_protocol::FsUnwatchParams;
use praxis_app_gateway_protocol::FsWatchParams;
use praxis_app_gateway_protocol::FsWriteFileParams;
use praxis_app_gateway_protocol::GetAccountParams;
use praxis_app_gateway_protocol::InitializeCapabilities;
use praxis_app_gateway_protocol::InitializeParams;
use praxis_app_gateway_protocol::JSONRPCError;
use praxis_app_gateway_protocol::JSONRPCErrorError;
use praxis_app_gateway_protocol::JSONRPCMessage;
use praxis_app_gateway_protocol::JSONRPCNotification;
use praxis_app_gateway_protocol::JSONRPCRequest;
use praxis_app_gateway_protocol::JSONRPCResponse;
use praxis_app_gateway_protocol::LoginAccountParams;
use praxis_app_gateway_protocol::MockExperimentalMethodParams;
use praxis_app_gateway_protocol::ModelListParams;
use praxis_app_gateway_protocol::PluginInstallParams;
use praxis_app_gateway_protocol::PluginListParams;
use praxis_app_gateway_protocol::PluginReadParams;
use praxis_app_gateway_protocol::PluginUninstallParams;
use praxis_app_gateway_protocol::RequestId;
use praxis_app_gateway_protocol::ReviewStartParams;
use praxis_app_gateway_protocol::ServerRequest;
use praxis_app_gateway_protocol::SkillsListParams;
use praxis_app_gateway_protocol::ThreadArchiveParams;
use praxis_app_gateway_protocol::ThreadCompactStartParams;
use praxis_app_gateway_protocol::ThreadForkParams;
use praxis_app_gateway_protocol::ThreadListParams;
use praxis_app_gateway_protocol::ThreadLoadedListParams;
use praxis_app_gateway_protocol::ThreadMetadataUpdateParams;
use praxis_app_gateway_protocol::ThreadReadParams;
use praxis_app_gateway_protocol::ThreadRealtimeAppendAudioParams;
use praxis_app_gateway_protocol::ThreadRealtimeAppendTextParams;
use praxis_app_gateway_protocol::ThreadRealtimeStartParams;
use praxis_app_gateway_protocol::ThreadRealtimeStopParams;
use praxis_app_gateway_protocol::ThreadResumeParams;
use praxis_app_gateway_protocol::ThreadRollbackParams;
use praxis_app_gateway_protocol::ThreadSetNameParams;
use praxis_app_gateway_protocol::ThreadShellCommandParams;
use praxis_app_gateway_protocol::ThreadStartParams;
use praxis_app_gateway_protocol::ThreadUnarchiveParams;
use praxis_app_gateway_protocol::ThreadUnsubscribeParams;
use praxis_app_gateway_protocol::TurnCompletedNotification;
use praxis_app_gateway_protocol::TurnInterruptParams;
use praxis_app_gateway_protocol::TurnStartParams;
use praxis_app_gateway_protocol::TurnSteerParams;
use praxis_app_gateway_protocol::WindowsSandboxSetupStartParams;
use praxis_login::default_client::PRAXIS_INTERNAL_ORIGINATOR_OVERRIDE_ENV_VAR;
use tokio::process::Command;

pub struct McpProcess {
    next_request_id: AtomicI64,
    /// Retain this child process until the client is dropped. The Tokio runtime
    /// will make a "best effort" to reap the process after it exits, but it is
    /// not a guarantee. See the `kill_on_drop` documentation for details.
    #[allow(dead_code)]
    process: Child,
    stdin: Option<ChildStdin>,
    stdout: BufReader<ChildStdout>,
    pending_messages: VecDeque<JSONRPCMessage>,
}

pub const DEFAULT_CLIENT_NAME: &str = "praxis-app-gateway-tests";

#[path = "mcp_process/requests.rs"]
mod requests;

impl McpProcess {
    pub async fn new(praxis_home: &Path) -> anyhow::Result<Self> {
        Self::new_with_env_and_args(praxis_home, &[], &[]).await
    }

    pub async fn new_with_args(praxis_home: &Path, args: &[&str]) -> anyhow::Result<Self> {
        Self::new_with_env_and_args(praxis_home, &[], args).await
    }

    /// Creates a new MCP process, allowing tests to override or remove
    /// specific environment variables for the child process only.
    ///
    /// Pass a tuple of (key, Some(value)) to set/override, or (key, None) to
    /// remove a variable from the child's environment.
    pub async fn new_with_env(
        praxis_home: &Path,
        env_overrides: &[(&str, Option<&str>)],
    ) -> anyhow::Result<Self> {
        Self::new_with_env_and_args(praxis_home, env_overrides, &[]).await
    }

    async fn new_with_env_and_args(
        praxis_home: &Path,
        env_overrides: &[(&str, Option<&str>)],
        args: &[&str],
    ) -> anyhow::Result<Self> {
        let program = praxis_utils_cargo_bin::cargo_bin("praxis-app-gateway")
            .context("should find binary for praxis-app-gateway")?;
        let mut cmd = Command::new(program);

        cmd.stdin(Stdio::piped());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());
        cmd.current_dir(praxis_home);
        cmd.env("CODEX_HOME", praxis_home);
        cmd.env("PRAXIS_HOME", praxis_home);
        cmd.env("RUST_LOG", "info");
        cmd.env_remove(PRAXIS_INTERNAL_ORIGINATOR_OVERRIDE_ENV_VAR);
        cmd.args(args);

        for (k, v) in env_overrides {
            match v {
                Some(val) => {
                    cmd.env(k, val);
                }
                None => {
                    cmd.env_remove(k);
                }
            }
        }

        let mut process = cmd
            .kill_on_drop(true)
            .spawn()
            .context("praxis-mcp-server proc should start")?;
        let stdin = process
            .stdin
            .take()
            .ok_or_else(|| anyhow::format_err!("mcp should have stdin fd"))?;
        let stdout = process
            .stdout
            .take()
            .ok_or_else(|| anyhow::format_err!("mcp should have stdout fd"))?;
        let stdout = BufReader::new(stdout);

        // Forward child's stderr to our stderr so failures are visible even
        // when stdout/stderr are captured by the test harness.
        if let Some(stderr) = process.stderr.take() {
            let mut stderr_reader = BufReader::new(stderr).lines();
            tokio::spawn(async move {
                while let Ok(Some(line)) = stderr_reader.next_line().await {
                    eprintln!("[mcp stderr] {line}");
                }
            });
        }
        Ok(Self {
            next_request_id: AtomicI64::new(0),
            process,
            stdin: Some(stdin),
            stdout,
            pending_messages: VecDeque::new(),
        })
    }

    /// Performs the initialization handshake with the MCP server.
    pub async fn initialize(&mut self) -> anyhow::Result<()> {
        let initialized = self
            .initialize_with_client_info(ClientInfo {
                name: DEFAULT_CLIENT_NAME.to_string(),
                title: None,
                version: "0.1.0".to_string(),
            })
            .await?;
        let JSONRPCMessage::Response(_) = initialized else {
            unreachable!("expected JSONRPCMessage::Response for initialize, got {initialized:?}");
        };
        Ok(())
    }

    /// Sends initialize with the provided client info and returns the response/error message.
    pub async fn initialize_with_client_info(
        &mut self,
        client_info: ClientInfo,
    ) -> anyhow::Result<JSONRPCMessage> {
        self.initialize_with_capabilities(
            client_info,
            Some(InitializeCapabilities {
                experimental_api: true,
                ..Default::default()
            }),
        )
        .await
    }

    pub async fn initialize_with_capabilities(
        &mut self,
        client_info: ClientInfo,
        capabilities: Option<InitializeCapabilities>,
    ) -> anyhow::Result<JSONRPCMessage> {
        self.initialize_with_params(InitializeParams {
            client_info,
            capabilities,
            host_extensions: Vec::new(),
        })
        .await
    }

    async fn initialize_with_params(
        &mut self,
        params: InitializeParams,
    ) -> anyhow::Result<JSONRPCMessage> {
        let params = Some(serde_json::to_value(params)?);
        let request_id = self.send_request("initialize", params).await?;
        let message = self.read_jsonrpc_message().await?;
        match message {
            JSONRPCMessage::Response(response) => {
                if response.id != RequestId::Integer(request_id) {
                    anyhow::bail!(
                        "initialize response id mismatch: expected {}, got {:?}",
                        request_id,
                        response.id
                    );
                }

                // Send notifications/initialized to ack the response.
                self.send_notification(ClientNotification::Initialized)
                    .await?;

                Ok(JSONRPCMessage::Response(response))
            }
            JSONRPCMessage::Error(error) => {
                if error.id != RequestId::Integer(request_id) {
                    anyhow::bail!(
                        "initialize error id mismatch: expected {}, got {:?}",
                        request_id,
                        error.id
                    );
                }
                Ok(JSONRPCMessage::Error(error))
            }
            JSONRPCMessage::Notification(notification) => {
                anyhow::bail!("unexpected JSONRPCMessage::Notification: {notification:?}");
            }
            JSONRPCMessage::Request(request) => {
                anyhow::bail!("unexpected JSONRPCMessage::Request: {request:?}");
            }
        }
    }

    /// Send an `account/rateLimits/read` JSON-RPC request.
    async fn send_request(
        &mut self,
        method: &str,
        params: Option<serde_json::Value>,
    ) -> anyhow::Result<i64> {
        let request_id = self.next_request_id.fetch_add(1, Ordering::Relaxed);

        let message = JSONRPCMessage::Request(JSONRPCRequest {
            id: RequestId::Integer(request_id),
            method: method.to_string(),
            params,
            trace: None,
        });
        self.send_jsonrpc_message(message).await?;
        Ok(request_id)
    }

    pub async fn send_response(
        &mut self,
        id: RequestId,
        result: serde_json::Value,
    ) -> anyhow::Result<()> {
        self.send_jsonrpc_message(JSONRPCMessage::Response(JSONRPCResponse { id, result }))
            .await
    }

    pub async fn send_error(
        &mut self,
        id: RequestId,
        error: JSONRPCErrorError,
    ) -> anyhow::Result<()> {
        self.send_jsonrpc_message(JSONRPCMessage::Error(JSONRPCError { id, error }))
            .await
    }

    pub async fn send_notification(
        &mut self,
        notification: ClientNotification,
    ) -> anyhow::Result<()> {
        let value = serde_json::to_value(notification)?;
        self.send_jsonrpc_message(JSONRPCMessage::Notification(JSONRPCNotification {
            method: value
                .get("method")
                .and_then(|m| m.as_str())
                .ok_or_else(|| anyhow::format_err!("notification missing method field"))?
                .to_string(),
            params: value.get("params").cloned(),
        }))
        .await
    }

    async fn send_jsonrpc_message(&mut self, message: JSONRPCMessage) -> anyhow::Result<()> {
        eprintln!("writing message to stdin: {message:?}");
        let Some(stdin) = self.stdin.as_mut() else {
            anyhow::bail!("mcp stdin closed");
        };
        let payload = serde_json::to_string(&message)?;
        stdin.write_all(payload.as_bytes()).await?;
        stdin.write_all(b"\n").await?;
        stdin.flush().await?;
        Ok(())
    }

    async fn read_jsonrpc_message(&mut self) -> anyhow::Result<JSONRPCMessage> {
        let mut line = String::new();
        self.stdout.read_line(&mut line).await?;
        let message = serde_json::from_str::<JSONRPCMessage>(&line)?;
        eprintln!("read message from stdout: {message:?}");
        Ok(message)
    }

    pub async fn read_stream_until_request_message(&mut self) -> anyhow::Result<ServerRequest> {
        eprintln!("in read_stream_until_request_message()");

        let message = self
            .read_stream_until_message(|message| matches!(message, JSONRPCMessage::Request(_)))
            .await?;

        let JSONRPCMessage::Request(jsonrpc_request) = message else {
            unreachable!("expected JSONRPCMessage::Request, got {message:?}");
        };
        jsonrpc_request
            .try_into()
            .with_context(|| "failed to deserialize ServerRequest from JSONRPCRequest")
    }

    pub async fn read_stream_until_response_message(
        &mut self,
        request_id: RequestId,
    ) -> anyhow::Result<JSONRPCResponse> {
        eprintln!("in read_stream_until_response_message({request_id:?})");

        let message = self
            .read_stream_until_message(|message| {
                Self::message_request_id(message) == Some(&request_id)
            })
            .await?;

        let JSONRPCMessage::Response(response) = message else {
            unreachable!("expected JSONRPCMessage::Response, got {message:?}");
        };
        Ok(response)
    }

    pub async fn read_stream_until_error_message(
        &mut self,
        request_id: RequestId,
    ) -> anyhow::Result<JSONRPCError> {
        let message = self
            .read_stream_until_message(|message| {
                Self::message_request_id(message) == Some(&request_id)
            })
            .await?;

        let JSONRPCMessage::Error(err) = message else {
            unreachable!("expected JSONRPCMessage::Error, got {message:?}");
        };
        Ok(err)
    }

    pub async fn read_stream_until_notification_message(
        &mut self,
        method: &str,
    ) -> anyhow::Result<JSONRPCNotification> {
        eprintln!("in read_stream_until_notification_message({method})");

        let message = self
            .read_stream_until_message(|message| {
                matches!(
                    message,
                    JSONRPCMessage::Notification(notification) if notification.method == method
                )
            })
            .await?;

        let JSONRPCMessage::Notification(notification) = message else {
            unreachable!("expected JSONRPCMessage::Notification, got {message:?}");
        };
        Ok(notification)
    }

    pub async fn read_stream_until_matching_notification<F>(
        &mut self,
        description: &str,
        predicate: F,
    ) -> anyhow::Result<JSONRPCNotification>
    where
        F: Fn(&JSONRPCNotification) -> bool,
    {
        eprintln!("in read_stream_until_matching_notification({description})");

        let message = self
            .read_stream_until_message(|message| {
                matches!(
                    message,
                    JSONRPCMessage::Notification(notification) if predicate(notification)
                )
            })
            .await?;

        let JSONRPCMessage::Notification(notification) = message else {
            unreachable!("expected JSONRPCMessage::Notification, got {message:?}");
        };
        Ok(notification)
    }

    pub async fn read_next_message(&mut self) -> anyhow::Result<JSONRPCMessage> {
        self.read_stream_until_message(|_| true).await
    }

    /// Clears any buffered messages so future reads only consider new stream items.
    ///
    /// We call this when e.g. we want to validate against the next turn and no longer care about
    /// messages buffered from the prior turn.
    pub fn clear_message_buffer(&mut self) {
        self.pending_messages.clear();
    }

    pub fn pending_notification_methods(&self) -> Vec<String> {
        self.pending_messages
            .iter()
            .filter_map(|message| match message {
                JSONRPCMessage::Notification(notification) => Some(notification.method.clone()),
                _ => None,
            })
            .collect()
    }

    /// Reads the stream until a message matches `predicate`, buffering any non-matching messages
    /// for later reads.
    async fn read_stream_until_message<F>(&mut self, predicate: F) -> anyhow::Result<JSONRPCMessage>
    where
        F: Fn(&JSONRPCMessage) -> bool,
    {
        if let Some(message) = self.take_pending_message(&predicate) {
            return Ok(message);
        }

        loop {
            let message = self.read_jsonrpc_message().await?;
            if predicate(&message) {
                return Ok(message);
            }
            self.pending_messages.push_back(message);
        }
    }

    fn take_pending_message<F>(&mut self, predicate: &F) -> Option<JSONRPCMessage>
    where
        F: Fn(&JSONRPCMessage) -> bool,
    {
        if let Some(pos) = self.pending_messages.iter().position(predicate) {
            return self.pending_messages.remove(pos);
        }
        None
    }

    fn pending_turn_completed_notification(&self, thread_id: &str, turn_id: &str) -> bool {
        self.pending_messages.iter().any(|message| {
            let JSONRPCMessage::Notification(notification) = message else {
                return false;
            };
            if notification.method != "turn/completed" {
                return false;
            }
            let Some(params) = notification.params.as_ref() else {
                return false;
            };
            let Ok(payload) = serde_json::from_value::<TurnCompletedNotification>(params.clone())
            else {
                return false;
            };
            payload.thread_id == thread_id && payload.turn.id == turn_id
        })
    }

    fn message_request_id(message: &JSONRPCMessage) -> Option<&RequestId> {
        match message {
            JSONRPCMessage::Request(request) => Some(&request.id),
            JSONRPCMessage::Response(response) => Some(&response.id),
            JSONRPCMessage::Error(err) => Some(&err.id),
            JSONRPCMessage::Notification(_) => None,
        }
    }
}

impl Drop for McpProcess {
    fn drop(&mut self) {
        // These tests spawn a `praxis-app-gateway` child process.
        //
        // We keep that child alive for the test and rely on Tokio's `kill_on_drop(true)` when this
        // helper is dropped. Tokio documents kill-on-drop as best-effort: dropping requests
        // termination, but it does not guarantee the child has fully exited and been reaped before
        // teardown continues.
        //
        // That makes cleanup timing nondeterministic. Leak detection can occasionally observe the
        // child still alive at teardown and report `LEAK`, which makes the test flaky.
        //
        // Drop can't be async, so we do a bounded synchronous cleanup:
        //
        // 1. Close stdin to request a graceful shutdown via EOF.
        // 2. Poll briefly for graceful exit.
        // 3. If still alive, request termination with `start_kill()`.
        // 4. Poll `try_wait()` until the OS reports the child exited, with a short timeout.
        drop(self.stdin.take());

        let graceful_start = std::time::Instant::now();
        let graceful_timeout = std::time::Duration::from_millis(200);
        while graceful_start.elapsed() < graceful_timeout {
            match self.process.try_wait() {
                Ok(Some(_)) => return,
                Ok(None) => std::thread::sleep(std::time::Duration::from_millis(5)),
                Err(_) => return,
            }
        }

        let _ = self.process.start_kill();

        let start = std::time::Instant::now();
        let timeout = std::time::Duration::from_secs(5);
        while start.elapsed() < timeout {
            match self.process.try_wait() {
                Ok(Some(_)) => return,
                Ok(None) => std::thread::sleep(std::time::Duration::from_millis(10)),
                Err(_) => return,
            }
        }
    }
}
