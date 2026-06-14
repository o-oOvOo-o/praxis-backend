use std::path::PathBuf;
use std::sync::Arc;

use praxis_app_gateway_protocol::ServerNotification;
use praxis_app_gateway_protocol::WindowsSandboxSetupCompletedNotification;
use praxis_app_gateway_protocol::WindowsSandboxSetupMode;
use praxis_app_gateway_protocol::WindowsSandboxSetupStartParams;
use praxis_app_gateway_protocol::WindowsSandboxSetupStartResponse;
use praxis_core::config::ConfigOverrides;
use praxis_core::windows_sandbox::WindowsSandboxSetupMode as CoreWindowsSandboxSetupMode;
use praxis_core::windows_sandbox::WindowsSandboxSetupRequest;

use super::PraxisMessageProcessor;
use super::derive_config_for_cwd;
use crate::outgoing_message::ConnectionRequestId;

impl PraxisMessageProcessor {
    pub(super) async fn windows_sandbox_setup_start(
        &mut self,
        request_id: ConnectionRequestId,
        params: WindowsSandboxSetupStartParams,
    ) {
        self.outgoing
            .send_response(
                request_id.clone(),
                WindowsSandboxSetupStartResponse { started: true },
            )
            .await;

        let mode = match params.mode {
            WindowsSandboxSetupMode::Elevated => CoreWindowsSandboxSetupMode::Elevated,
            WindowsSandboxSetupMode::Unelevated => CoreWindowsSandboxSetupMode::Unelevated,
        };
        let config = Arc::clone(&self.config);
        let cloud_requirements = self.current_cloud_requirements();
        let command_cwd = params
            .cwd
            .map(PathBuf::from)
            .unwrap_or_else(|| config.cwd.to_path_buf());
        let cli_overrides = self.current_cli_overrides();
        let runtime_feature_enablement = self.current_runtime_feature_enablement();
        let outgoing = Arc::clone(&self.outgoing);
        let connection_id = request_id.connection_id;

        tokio::spawn(async move {
            let derived_config = derive_config_for_cwd(
                &cli_overrides,
                /*request_overrides*/ None,
                ConfigOverrides {
                    cwd: Some(command_cwd.clone()),
                    ..Default::default()
                },
                Some(command_cwd.clone()),
                &cloud_requirements,
                &config.praxis_home,
                &runtime_feature_enablement,
            )
            .await;
            let setup_result = match derived_config {
                Ok(config) => {
                    let setup_request = WindowsSandboxSetupRequest {
                        mode,
                        policy: config.permissions.sandbox_policy.get().clone(),
                        policy_cwd: config.cwd.to_path_buf(),
                        command_cwd,
                        env_map: std::env::vars().collect(),
                        praxis_home: config.praxis_home.clone(),
                        active_profile: config.active_profile.clone(),
                    };
                    praxis_core::windows_sandbox::run_windows_sandbox_setup(setup_request).await
                }
                Err(err) => Err(err.into()),
            };
            let notification = WindowsSandboxSetupCompletedNotification {
                mode: match mode {
                    CoreWindowsSandboxSetupMode::Elevated => WindowsSandboxSetupMode::Elevated,
                    CoreWindowsSandboxSetupMode::Unelevated => WindowsSandboxSetupMode::Unelevated,
                },
                success: setup_result.is_ok(),
                error: setup_result.err().map(|err| err.to_string()),
            };
            outgoing
                .send_server_notification_to_connections(
                    &[connection_id],
                    ServerNotification::WindowsSandboxSetupCompleted(notification),
                )
                .await;
        });
    }
}
