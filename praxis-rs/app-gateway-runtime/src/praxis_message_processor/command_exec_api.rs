use praxis_app_gateway_protocol::CommandExecParams;
use praxis_app_gateway_protocol::CommandExecResizeParams;
use praxis_app_gateway_protocol::CommandExecTerminateParams;
use praxis_app_gateway_protocol::CommandExecWriteParams;
use praxis_app_gateway_protocol::JSONRPCErrorError;
use praxis_core::config::NetworkProxyAuditMetadata;
use praxis_core::exec::ExecCapturePolicy;
use praxis_core::exec::ExecExpiration;
use praxis_core::exec::ExecParams;
use praxis_core::exec_env::create_env;
use praxis_core::sandboxing::SandboxPermissions;
use praxis_core::windows_sandbox::WindowsSandboxLevelExt;
use praxis_protocol::config_types::WindowsSandboxLevel;
use praxis_utils_pty::DEFAULT_OUTPUT_BYTES_CAP;
use tokio_util::sync::CancellationToken;

use super::PraxisMessageProcessor;
use crate::command_exec::StartCommandExecParams;
use crate::command_exec::terminal_size_from_protocol;
use crate::error_code::INTERNAL_ERROR_CODE;
use crate::error_code::INVALID_PARAMS_ERROR_CODE;
use crate::error_code::INVALID_REQUEST_ERROR_CODE;
use crate::outgoing_message::ConnectionRequestId;

impl PraxisMessageProcessor {
    pub(super) async fn exec_one_off_command(
        &self,
        request_id: ConnectionRequestId,
        params: CommandExecParams,
    ) {
        tracing::debug!("ExecOneOffCommand params: {params:?}");

        let request = request_id.clone();

        if params.command.is_empty() {
            let error = JSONRPCErrorError {
                code: INVALID_REQUEST_ERROR_CODE,
                message: "command must not be empty".to_string(),
                data: None,
            };
            self.outgoing.send_error(request, error).await;
            return;
        }

        let CommandExecParams {
            command,
            process_id,
            tty,
            stream_stdin,
            stream_stdout_stderr,
            output_bytes_cap,
            disable_output_cap,
            disable_timeout,
            timeout_ms,
            cwd,
            env: env_overrides,
            size,
            sandbox_policy,
        } = params;

        if size.is_some() && !tty {
            let error = JSONRPCErrorError {
                code: INVALID_PARAMS_ERROR_CODE,
                message: "command/exec size requires tty: true".to_string(),
                data: None,
            };
            self.outgoing.send_error(request, error).await;
            return;
        }

        if disable_output_cap && output_bytes_cap.is_some() {
            let error = JSONRPCErrorError {
                code: INVALID_PARAMS_ERROR_CODE,
                message: "command/exec cannot set both outputBytesCap and disableOutputCap"
                    .to_string(),
                data: None,
            };
            self.outgoing.send_error(request, error).await;
            return;
        }

        if disable_timeout && timeout_ms.is_some() {
            let error = JSONRPCErrorError {
                code: INVALID_PARAMS_ERROR_CODE,
                message: "command/exec cannot set both timeoutMs and disableTimeout".to_string(),
                data: None,
            };
            self.outgoing.send_error(request, error).await;
            return;
        }

        let cwd = cwd.unwrap_or_else(|| self.config.cwd.to_path_buf());
        let mut env = create_env(
            &self.config.permissions.shell_environment_policy,
            /*thread_id*/ None,
        );
        if let Some(env_overrides) = env_overrides {
            for (key, value) in env_overrides {
                match value {
                    Some(value) => {
                        env.insert(key, value);
                    }
                    None => {
                        env.remove(&key);
                    }
                }
            }
        }
        let timeout_ms = match timeout_ms {
            Some(timeout_ms) => match u64::try_from(timeout_ms) {
                Ok(timeout_ms) => Some(timeout_ms),
                Err(_) => {
                    let error = JSONRPCErrorError {
                        code: INVALID_PARAMS_ERROR_CODE,
                        message: format!(
                            "command/exec timeoutMs must be non-negative, got {timeout_ms}"
                        ),
                        data: None,
                    };
                    self.outgoing.send_error(request, error).await;
                    return;
                }
            },
            None => None,
        };
        let managed_network_requirements_enabled =
            self.config.managed_network_requirements_enabled();
        let started_network_proxy = match self.config.permissions.network.as_ref() {
            Some(spec) => match spec
                .start_proxy(
                    self.config.permissions.sandbox_policy.get(),
                    /*policy_decider*/ None,
                    /*blocked_request_observer*/ None,
                    managed_network_requirements_enabled,
                    NetworkProxyAuditMetadata::default(),
                )
                .await
            {
                Ok(started) => Some(started),
                Err(err) => {
                    let error = JSONRPCErrorError {
                        code: INTERNAL_ERROR_CODE,
                        message: format!("failed to start managed network proxy: {err}"),
                        data: None,
                    };
                    self.outgoing.send_error(request, error).await;
                    return;
                }
            },
            None => None,
        };
        let windows_sandbox_level = WindowsSandboxLevel::from_config(&self.config);
        let output_bytes_cap = if disable_output_cap {
            None
        } else {
            Some(output_bytes_cap.unwrap_or(DEFAULT_OUTPUT_BYTES_CAP))
        };
        let expiration = if disable_timeout {
            ExecExpiration::Cancellation(CancellationToken::new())
        } else {
            match timeout_ms {
                Some(timeout_ms) => timeout_ms.into(),
                None => ExecExpiration::DefaultTimeout,
            }
        };
        let capture_policy = if disable_output_cap {
            ExecCapturePolicy::FullBuffer
        } else {
            ExecCapturePolicy::ShellTool
        };
        let sandbox_cwd = self.config.cwd.clone();
        let exec_params = ExecParams {
            command,
            cwd: cwd.clone(),
            expiration,
            capture_policy,
            env,
            network: started_network_proxy
                .as_ref()
                .map(praxis_core::config::StartedNetworkProxy::proxy),
            sandbox_permissions: SandboxPermissions::UseDefault,
            windows_sandbox_level,
            windows_sandbox_private_desktop: self
                .config
                .permissions
                .windows_sandbox_private_desktop,
            justification: None,
            arg0: None,
        };

        let requested_policy = sandbox_policy.map(|policy| policy.to_core());
        let (
            effective_policy,
            effective_file_system_sandbox_policy,
            effective_network_sandbox_policy,
        ) = match requested_policy {
            Some(policy) => match self.config.permissions.sandbox_policy.can_set(&policy) {
                Ok(()) => {
                    let file_system_sandbox_policy =
                        praxis_protocol::permissions::FileSystemSandboxPolicy::from_legacy_sandbox_policy(&policy, &sandbox_cwd);
                    let network_sandbox_policy =
                        praxis_protocol::permissions::NetworkSandboxPolicy::from(&policy);
                    (policy, file_system_sandbox_policy, network_sandbox_policy)
                }
                Err(err) => {
                    let error = JSONRPCErrorError {
                        code: INVALID_REQUEST_ERROR_CODE,
                        message: format!("invalid sandbox policy: {err}"),
                        data: None,
                    };
                    self.outgoing.send_error(request, error).await;
                    return;
                }
            },
            None => (
                self.config.permissions.sandbox_policy.get().clone(),
                self.config.permissions.file_system_sandbox_policy.clone(),
                self.config.permissions.network_sandbox_policy,
            ),
        };

        let praxis_linux_sandbox_exe = self.arg0_paths.praxis_linux_sandbox_exe.clone();
        let outgoing = self.outgoing.clone();
        let request_for_task = request.clone();
        let started_network_proxy_for_task = started_network_proxy;
        let use_legacy_landlock = self.config.features.use_legacy_landlock();
        let size = match size.map(terminal_size_from_protocol) {
            Some(Ok(size)) => Some(size),
            Some(Err(error)) => {
                self.outgoing.send_error(request, error).await;
                return;
            }
            None => None,
        };

        match praxis_core::exec::build_exec_request(
            exec_params,
            &effective_policy,
            &effective_file_system_sandbox_policy,
            effective_network_sandbox_policy,
            sandbox_cwd.as_path(),
            &praxis_linux_sandbox_exe,
            use_legacy_landlock,
        ) {
            Ok(exec_request) => {
                if let Err(error) = self
                    .command_exec_manager
                    .start(StartCommandExecParams {
                        outgoing,
                        request_id: request_for_task,
                        process_id,
                        exec_request,
                        started_network_proxy: started_network_proxy_for_task,
                        tty,
                        stream_stdin,
                        stream_stdout_stderr,
                        output_bytes_cap,
                        size,
                    })
                    .await
                {
                    self.outgoing.send_error(request, error).await;
                }
            }
            Err(err) => {
                let error = JSONRPCErrorError {
                    code: INTERNAL_ERROR_CODE,
                    message: format!("exec failed: {err}"),
                    data: None,
                };
                self.outgoing.send_error(request, error).await;
            }
        }
    }

    pub(super) async fn command_exec_write(
        &self,
        request_id: ConnectionRequestId,
        params: CommandExecWriteParams,
    ) {
        match self
            .command_exec_manager
            .write(request_id.clone(), params)
            .await
        {
            Ok(response) => self.outgoing.send_response(request_id, response).await,
            Err(error) => self.outgoing.send_error(request_id, error).await,
        }
    }

    pub(super) async fn command_exec_resize(
        &self,
        request_id: ConnectionRequestId,
        params: CommandExecResizeParams,
    ) {
        match self
            .command_exec_manager
            .resize(request_id.clone(), params)
            .await
        {
            Ok(response) => self.outgoing.send_response(request_id, response).await,
            Err(error) => self.outgoing.send_error(request_id, error).await,
        }
    }

    pub(super) async fn command_exec_terminate(
        &self,
        request_id: ConnectionRequestId,
        params: CommandExecTerminateParams,
    ) {
        match self
            .command_exec_manager
            .terminate(request_id.clone(), params)
            .await
        {
            Ok(response) => self.outgoing.send_response(request_id, response).await,
            Err(error) => self.outgoing.send_error(request_id, error).await,
        }
    }
}
