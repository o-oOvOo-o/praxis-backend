use std::collections::HashMap;
use std::io::Error as IoError;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::RwLock;
use std::time::Duration;

use praxis_app_gateway_protocol::Account;
use praxis_app_gateway_protocol::AccountLoginCompletedNotification;
use praxis_app_gateway_protocol::AccountUpdatedNotification;
use praxis_app_gateway_protocol::AuthMode;
use praxis_app_gateway_protocol::CancelLoginAccountParams;
use praxis_app_gateway_protocol::CancelLoginAccountResponse;
use praxis_app_gateway_protocol::CancelLoginAccountStatus;
use praxis_app_gateway_protocol::GetAccountParams;
use praxis_app_gateway_protocol::GetAccountRateLimitsResponse;
use praxis_app_gateway_protocol::GetAccountResponse;
use praxis_app_gateway_protocol::JSONRPCErrorError;
use praxis_app_gateway_protocol::LoginAccountParams;
use praxis_app_gateway_protocol::LoginAccountResponse;
use praxis_app_gateway_protocol::LogoutAccountResponse;
use praxis_app_gateway_protocol::ServerNotification;
use praxis_backend_client::Client as BackendClient;
use praxis_cloud_requirements::cloud_config_bundle_loader;
use praxis_core::config_loader::CloudConfigBundleLoader;
use praxis_login::AuthManager;
use praxis_login::AuthMode as CoreAuthMode;
use praxis_login::CLIENT_ID;
use praxis_login::CodexAuth;
use praxis_login::ServerOptions as LoginServerOptions;
use praxis_login::ShutdownHandle;
use praxis_login::auth::login_with_chatgpt_auth_tokens;
use praxis_login::complete_device_code_login;
use praxis_login::default_client::set_default_client_residency_requirement;
use praxis_login::login_with_api_key;
use praxis_login::request_device_code;
use praxis_login::run_login_server;
use praxis_protocol::config_types::ForcedLoginMethod;
use praxis_protocol::protocol::RateLimitSnapshot as CoreRateLimitSnapshot;
use tokio_util::sync::CancellationToken;
use toml::Value as TomlValue;
use tracing::warn;
use uuid::Uuid;

use super::PraxisMessageProcessor;
use crate::error_code::INTERNAL_ERROR_CODE;
use crate::error_code::INVALID_REQUEST_ERROR_CODE;
use crate::outgoing_message::ConnectionRequestId;

const LOGIN_CHATGPT_TIMEOUT: Duration = Duration::from_secs(10 * 60);
const LOGIN_ISSUER_OVERRIDE_ENV_VAR: &str = "PRAXIS_APP_GATEWAY_LOGIN_ISSUER";

pub(super) enum ActiveLogin {
    Browser {
        shutdown_handle: ShutdownHandle,
        login_id: Uuid,
    },
    DeviceCode {
        cancel: CancellationToken,
        login_id: Uuid,
    },
}

impl ActiveLogin {
    fn login_id(&self) -> Uuid {
        match self {
            ActiveLogin::Browser { login_id, .. } | ActiveLogin::DeviceCode { login_id, .. } => {
                *login_id
            }
        }
    }

    fn cancel(&self) {
        match self {
            ActiveLogin::Browser {
                shutdown_handle, ..
            } => shutdown_handle.shutdown(),
            ActiveLogin::DeviceCode { cancel, .. } => cancel.cancel(),
        }
    }
}

impl Drop for ActiveLogin {
    fn drop(&mut self) {
        self.cancel();
    }
}

#[derive(Clone, Copy, Debug)]
enum CancelLoginError {
    NotFound,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RefreshTokenRequestOutcome {
    NotAttemptedOrSucceeded,
    FailedTransiently,
    FailedPermanently,
}

impl PraxisMessageProcessor {
    fn current_account_updated_notification(&self) -> AccountUpdatedNotification {
        let auth = self.auth_manager.auth_cached();
        AccountUpdatedNotification {
            auth_mode: auth.as_ref().map(CodexAuth::api_auth_mode),
            plan_type: auth.as_ref().and_then(CodexAuth::account_plan_type),
        }
    }

    pub(super) async fn login_account(
        &mut self,
        request_id: ConnectionRequestId,
        params: LoginAccountParams,
    ) {
        match params {
            LoginAccountParams::ApiKey { api_key } => {
                self.login_api_key(request_id, api_key).await;
            }
            LoginAccountParams::Chatgpt => {
                self.login_chatgpt(request_id).await;
            }
            LoginAccountParams::ChatgptDeviceCode => {
                self.login_chatgpt_device_code(request_id).await;
            }
            LoginAccountParams::ChatgptAuthTokens {
                access_token,
                chatgpt_account_id,
                chatgpt_plan_type,
            } => {
                self.login_chatgpt_auth_tokens(
                    request_id,
                    access_token,
                    chatgpt_account_id,
                    chatgpt_plan_type,
                )
                .await;
            }
        }
    }

    fn external_auth_active_error(&self) -> JSONRPCErrorError {
        JSONRPCErrorError {
            code: INVALID_REQUEST_ERROR_CODE,
            message: "External auth is active. Use account/login/start (chatgptAuthTokens) to update it or account/logout to clear it."
                .to_string(),
            data: None,
        }
    }

    async fn login_api_key_common(
        &mut self,
        api_key: &str,
    ) -> std::result::Result<(), JSONRPCErrorError> {
        if self.auth_manager.is_external_chatgpt_auth_active() {
            return Err(self.external_auth_active_error());
        }

        if matches!(
            self.config.forced_login_method,
            Some(ForcedLoginMethod::Chatgpt)
        ) {
            return Err(JSONRPCErrorError {
                code: INVALID_REQUEST_ERROR_CODE,
                message: "API key login is disabled. Use ChatGPT login instead.".to_string(),
                data: None,
            });
        }

        {
            let mut guard = self.active_login.lock().await;
            if let Some(active) = guard.take() {
                drop(active);
            }
        }

        match login_with_api_key(
            &self.config.praxis_home,
            api_key,
            self.config.cli_auth_credentials_store_mode,
        ) {
            Ok(()) => {
                self.auth_manager.reload();
                Ok(())
            }
            Err(err) => Err(JSONRPCErrorError {
                code: INTERNAL_ERROR_CODE,
                message: format!("failed to save api key: {err}"),
                data: None,
            }),
        }
    }

    async fn login_api_key(&mut self, request_id: ConnectionRequestId, api_key: String) {
        match self.login_api_key_common(&api_key).await {
            Ok(()) => {
                self.outgoing
                    .send_response(request_id, LoginAccountResponse::ApiKey {})
                    .await;

                let payload_login_completed = AccountLoginCompletedNotification {
                    login_id: None,
                    success: true,
                    error: None,
                };
                self.outgoing
                    .send_server_notification(ServerNotification::AccountLoginCompleted(
                        payload_login_completed,
                    ))
                    .await;

                self.outgoing
                    .send_server_notification(ServerNotification::AccountUpdated(
                        self.current_account_updated_notification(),
                    ))
                    .await;
            }
            Err(error) => {
                self.outgoing.send_error(request_id, error).await;
            }
        }
    }

    async fn login_chatgpt_common(
        &self,
    ) -> std::result::Result<LoginServerOptions, JSONRPCErrorError> {
        let config = self.config.as_ref();

        if self.auth_manager.is_external_chatgpt_auth_active() {
            return Err(self.external_auth_active_error());
        }

        if matches!(config.forced_login_method, Some(ForcedLoginMethod::Api)) {
            return Err(JSONRPCErrorError {
                code: INVALID_REQUEST_ERROR_CODE,
                message: "ChatGPT login is disabled. Use API key login instead.".to_string(),
                data: None,
            });
        }

        let mut opts = LoginServerOptions {
            open_browser: false,
            ..LoginServerOptions::new(
                config.praxis_home.clone(),
                CLIENT_ID.to_string(),
                config.forced_chatgpt_workspace_id.clone(),
                config.cli_auth_credentials_store_mode,
            )
        };
        #[cfg(debug_assertions)]
        if let Ok(issuer) = std::env::var(LOGIN_ISSUER_OVERRIDE_ENV_VAR)
            && !issuer.trim().is_empty()
        {
            opts.issuer = issuer;
        }

        Ok(opts)
    }

    fn login_chatgpt_device_code_start_error(err: IoError) -> JSONRPCErrorError {
        let is_not_found = err.kind() == std::io::ErrorKind::NotFound;
        JSONRPCErrorError {
            code: if is_not_found {
                INVALID_REQUEST_ERROR_CODE
            } else {
                INTERNAL_ERROR_CODE
            },
            message: if is_not_found {
                err.to_string()
            } else {
                format!("failed to request device code: {err}")
            },
            data: None,
        }
    }

    async fn login_chatgpt(&mut self, request_id: ConnectionRequestId) {
        match self.login_chatgpt_common().await {
            Ok(opts) => match run_login_server(opts) {
                Ok(server) => {
                    let login_id = Uuid::new_v4();
                    let shutdown_handle = server.cancel_handle();

                    {
                        let mut guard = self.active_login.lock().await;
                        if let Some(existing) = guard.take() {
                            drop(existing);
                        }
                        *guard = Some(ActiveLogin::Browser {
                            shutdown_handle: shutdown_handle.clone(),
                            login_id,
                        });
                    }

                    let outgoing_clone = self.outgoing.clone();
                    let active_login = self.active_login.clone();
                    let auth_manager = self.auth_manager.clone();
                    let cloud_requirements = self.cloud_requirements.clone();
                    let chatgpt_base_url = self.config.chatgpt_base_url.clone();
                    let praxis_home = self.config.praxis_home.clone();
                    let cli_overrides = self.current_cli_overrides();
                    let auth_url = server.auth_url.clone();
                    tokio::spawn(async move {
                        let (success, error_msg) = match tokio::time::timeout(
                            LOGIN_CHATGPT_TIMEOUT,
                            server.block_until_done(),
                        )
                        .await
                        {
                            Ok(Ok(())) => (true, None),
                            Ok(Err(err)) => (false, Some(format!("Login server error: {err}"))),
                            Err(_elapsed) => {
                                shutdown_handle.shutdown();
                                (false, Some("Login timed out".to_string()))
                            }
                        };

                        let payload = AccountLoginCompletedNotification {
                            login_id: Some(login_id.to_string()),
                            success,
                            error: error_msg,
                        };
                        outgoing_clone
                            .send_server_notification(ServerNotification::AccountLoginCompleted(
                                payload,
                            ))
                            .await;

                        if success {
                            auth_manager.reload();
                            replace_cloud_config_bundle_loader(
                                cloud_requirements.as_ref(),
                                auth_manager.clone(),
                                chatgpt_base_url,
                                praxis_home,
                            );
                            sync_default_client_residency_requirement(
                                &cli_overrides,
                                cloud_requirements.as_ref(),
                            )
                            .await;

                            let auth = auth_manager.auth_cached();
                            let payload = AccountUpdatedNotification {
                                auth_mode: auth.as_ref().map(CodexAuth::api_auth_mode),
                                plan_type: auth.as_ref().and_then(CodexAuth::account_plan_type),
                            };
                            outgoing_clone
                                .send_server_notification(ServerNotification::AccountUpdated(
                                    payload,
                                ))
                                .await;
                        }

                        let mut guard = active_login.lock().await;
                        if guard.as_ref().map(ActiveLogin::login_id) == Some(login_id) {
                            *guard = None;
                        }
                    });

                    let response = LoginAccountResponse::Chatgpt {
                        login_id: login_id.to_string(),
                        auth_url,
                    };
                    self.outgoing.send_response(request_id, response).await;
                }
                Err(err) => {
                    let error = JSONRPCErrorError {
                        code: INTERNAL_ERROR_CODE,
                        message: format!("failed to start login server: {err}"),
                        data: None,
                    };
                    self.outgoing.send_error(request_id, error).await;
                }
            },
            Err(err) => {
                self.outgoing.send_error(request_id, err).await;
            }
        }
    }

    async fn login_chatgpt_device_code(&mut self, request_id: ConnectionRequestId) {
        match self.login_chatgpt_common().await {
            Ok(opts) => match request_device_code(&opts).await {
                Ok(device_code) => {
                    let login_id = Uuid::new_v4();
                    let cancel = CancellationToken::new();

                    {
                        let mut guard = self.active_login.lock().await;
                        if let Some(existing) = guard.take() {
                            drop(existing);
                        }
                        *guard = Some(ActiveLogin::DeviceCode {
                            cancel: cancel.clone(),
                            login_id,
                        });
                    }

                    let verification_url = device_code.verification_url.clone();
                    let user_code = device_code.user_code.clone();
                    let response = LoginAccountResponse::ChatgptDeviceCode {
                        login_id: login_id.to_string(),
                        verification_url,
                        user_code,
                    };
                    self.outgoing.send_response(request_id, response).await;

                    let outgoing_clone = self.outgoing.clone();
                    let active_login = self.active_login.clone();
                    let auth_manager = self.auth_manager.clone();
                    let cloud_requirements = self.cloud_requirements.clone();
                    let chatgpt_base_url = self.config.chatgpt_base_url.clone();
                    let praxis_home = self.config.praxis_home.clone();
                    let cli_overrides = self.current_cli_overrides();
                    tokio::spawn(async move {
                        let (success, error_msg) = tokio::select! {
                            _ = cancel.cancelled() => {
                                (false, Some("Login was not completed".to_string()))
                            }
                            r = complete_device_code_login(opts, device_code) => {
                                match r {
                                    Ok(()) => (true, None),
                                    Err(err) => (false, Some(err.to_string())),
                                }
                            }
                        };

                        let payload = AccountLoginCompletedNotification {
                            login_id: Some(login_id.to_string()),
                            success,
                            error: error_msg,
                        };
                        outgoing_clone
                            .send_server_notification(ServerNotification::AccountLoginCompleted(
                                payload,
                            ))
                            .await;

                        if success {
                            auth_manager.reload();
                            replace_cloud_config_bundle_loader(
                                cloud_requirements.as_ref(),
                                auth_manager.clone(),
                                chatgpt_base_url,
                                praxis_home,
                            );
                            sync_default_client_residency_requirement(
                                &cli_overrides,
                                cloud_requirements.as_ref(),
                            )
                            .await;

                            let auth = auth_manager.auth_cached();
                            let payload = AccountUpdatedNotification {
                                auth_mode: auth.as_ref().map(CodexAuth::api_auth_mode),
                                plan_type: auth.as_ref().and_then(CodexAuth::account_plan_type),
                            };
                            outgoing_clone
                                .send_server_notification(ServerNotification::AccountUpdated(
                                    payload,
                                ))
                                .await;
                        }

                        let mut guard = active_login.lock().await;
                        if guard.as_ref().map(ActiveLogin::login_id) == Some(login_id) {
                            *guard = None;
                        }
                    });
                }
                Err(err) => {
                    let error = Self::login_chatgpt_device_code_start_error(err);
                    self.outgoing.send_error(request_id, error).await;
                }
            },
            Err(err) => {
                self.outgoing.send_error(request_id, err).await;
            }
        }
    }

    async fn cancel_login_chatgpt_common(
        &mut self,
        login_id: Uuid,
    ) -> std::result::Result<(), CancelLoginError> {
        let mut guard = self.active_login.lock().await;
        if guard.as_ref().map(ActiveLogin::login_id) == Some(login_id) {
            if let Some(active) = guard.take() {
                drop(active);
            }
            Ok(())
        } else {
            Err(CancelLoginError::NotFound)
        }
    }

    pub(super) async fn cancel_login_account(
        &mut self,
        request_id: ConnectionRequestId,
        params: CancelLoginAccountParams,
    ) {
        let login_id = params.login_id;
        match Uuid::parse_str(&login_id) {
            Ok(uuid) => {
                let status = match self.cancel_login_chatgpt_common(uuid).await {
                    Ok(()) => CancelLoginAccountStatus::Canceled,
                    Err(CancelLoginError::NotFound) => CancelLoginAccountStatus::NotFound,
                };
                let response = CancelLoginAccountResponse { status };
                self.outgoing.send_response(request_id, response).await;
            }
            Err(_) => {
                let error = JSONRPCErrorError {
                    code: INVALID_REQUEST_ERROR_CODE,
                    message: format!("invalid login id: {login_id}"),
                    data: None,
                };
                self.outgoing.send_error(request_id, error).await;
            }
        }
    }

    async fn login_chatgpt_auth_tokens(
        &mut self,
        request_id: ConnectionRequestId,
        access_token: String,
        chatgpt_account_id: String,
        chatgpt_plan_type: Option<String>,
    ) {
        if matches!(
            self.config.forced_login_method,
            Some(ForcedLoginMethod::Api)
        ) {
            let error = JSONRPCErrorError {
                code: INVALID_REQUEST_ERROR_CODE,
                message: "External ChatGPT auth is disabled. Use API key login instead."
                    .to_string(),
                data: None,
            };
            self.outgoing.send_error(request_id, error).await;
            return;
        }

        {
            let mut guard = self.active_login.lock().await;
            if let Some(active) = guard.take() {
                drop(active);
            }
        }

        if let Some(expected_workspace) = self.config.forced_chatgpt_workspace_id.as_deref()
            && chatgpt_account_id != expected_workspace
        {
            let error = JSONRPCErrorError {
                code: INVALID_REQUEST_ERROR_CODE,
                message: format!(
                    "External auth must use workspace {expected_workspace}, but received {chatgpt_account_id:?}."
                ),
                data: None,
            };
            self.outgoing.send_error(request_id, error).await;
            return;
        }

        if let Err(err) = login_with_chatgpt_auth_tokens(
            &self.config.praxis_home,
            &access_token,
            &chatgpt_account_id,
            chatgpt_plan_type.as_deref(),
        ) {
            let error = JSONRPCErrorError {
                code: INTERNAL_ERROR_CODE,
                message: format!("failed to set external auth: {err}"),
                data: None,
            };
            self.outgoing.send_error(request_id, error).await;
            return;
        }
        self.auth_manager.reload();
        replace_cloud_config_bundle_loader(
            self.cloud_requirements.as_ref(),
            self.auth_manager.clone(),
            self.config.chatgpt_base_url.clone(),
            self.config.praxis_home.clone(),
        );
        let cli_overrides = self.current_cli_overrides();
        sync_default_client_residency_requirement(&cli_overrides, self.cloud_requirements.as_ref())
            .await;

        self.outgoing
            .send_response(request_id, LoginAccountResponse::ChatgptAuthTokens {})
            .await;

        let payload_login_completed = AccountLoginCompletedNotification {
            login_id: None,
            success: true,
            error: None,
        };
        self.outgoing
            .send_server_notification(ServerNotification::AccountLoginCompleted(
                payload_login_completed,
            ))
            .await;

        self.outgoing
            .send_server_notification(ServerNotification::AccountUpdated(
                self.current_account_updated_notification(),
            ))
            .await;
    }

    async fn logout_common(&mut self) -> std::result::Result<Option<AuthMode>, JSONRPCErrorError> {
        {
            let mut guard = self.active_login.lock().await;
            if let Some(active) = guard.take() {
                drop(active);
            }
        }

        if let Err(err) = self.auth_manager.logout() {
            return Err(JSONRPCErrorError {
                code: INTERNAL_ERROR_CODE,
                message: format!("logout failed: {err}"),
                data: None,
            });
        }

        Ok(self
            .auth_manager
            .auth_cached()
            .as_ref()
            .map(CodexAuth::api_auth_mode))
    }

    pub(super) async fn logout_account(&mut self, request_id: ConnectionRequestId) {
        match self.logout_common().await {
            Ok(current_auth_method) => {
                self.outgoing
                    .send_response(request_id, LogoutAccountResponse {})
                    .await;

                let payload = AccountUpdatedNotification {
                    auth_mode: current_auth_method,
                    plan_type: None,
                };
                self.outgoing
                    .send_server_notification(ServerNotification::AccountUpdated(payload))
                    .await;
            }
            Err(error) => {
                self.outgoing.send_error(request_id, error).await;
            }
        }
    }

    async fn refresh_token_if_requested(&self, do_refresh: bool) -> RefreshTokenRequestOutcome {
        if self.auth_manager.is_external_chatgpt_auth_active() {
            return RefreshTokenRequestOutcome::NotAttemptedOrSucceeded;
        }
        if do_refresh && let Err(err) = self.auth_manager.refresh_token().await {
            let failed_reason = err.failed_reason();
            if failed_reason.is_none() {
                tracing::warn!("failed to refresh token while getting account: {err}");
                return RefreshTokenRequestOutcome::FailedTransiently;
            }
            return RefreshTokenRequestOutcome::FailedPermanently;
        }
        RefreshTokenRequestOutcome::NotAttemptedOrSucceeded
    }

    pub(super) async fn get_account(
        &self,
        request_id: ConnectionRequestId,
        params: GetAccountParams,
    ) {
        let do_refresh = params.refresh_token;

        self.refresh_token_if_requested(do_refresh).await;

        let requires_openai_auth = self.config.model_provider.requires_openai_auth;

        if !requires_openai_auth {
            let response = GetAccountResponse {
                account: None,
                requires_openai_auth,
            };
            self.outgoing.send_response(request_id, response).await;
            return;
        }

        let account = match self.auth_manager.auth_cached() {
            Some(auth) => match auth.auth_mode() {
                CoreAuthMode::ApiKey => Some(Account::ApiKey {}),
                CoreAuthMode::Chatgpt | CoreAuthMode::ChatgptAuthTokens => {
                    let email = auth.get_account_email();
                    let plan_type = auth.account_plan_type();

                    match (email, plan_type) {
                        (Some(email), Some(plan_type)) => {
                            Some(Account::Chatgpt { email, plan_type })
                        }
                        _ => {
                            let error = JSONRPCErrorError {
                                code: INVALID_REQUEST_ERROR_CODE,
                                message:
                                    "email and plan type are required for chatgpt authentication"
                                        .to_string(),
                                data: None,
                            };
                            self.outgoing.send_error(request_id, error).await;
                            return;
                        }
                    }
                }
            },
            None => None,
        };

        let response = GetAccountResponse {
            account,
            requires_openai_auth,
        };
        self.outgoing.send_response(request_id, response).await;
    }

    pub(super) async fn get_account_rate_limits(&self, request_id: ConnectionRequestId) {
        match self.fetch_account_rate_limits().await {
            Ok(rate_limits) => {
                let response = GetAccountRateLimitsResponse {
                    rate_limits: rate_limits
                        .into_iter()
                        .map(|(limit_id, snapshot)| (limit_id, snapshot.into()))
                        .collect(),
                };
                self.outgoing.send_response(request_id, response).await;
            }
            Err(error) => {
                self.outgoing.send_error(request_id, error).await;
            }
        }
    }

    async fn fetch_account_rate_limits(
        &self,
    ) -> Result<HashMap<String, CoreRateLimitSnapshot>, JSONRPCErrorError> {
        let Some(auth) = self.auth_manager.auth().await else {
            return Err(JSONRPCErrorError {
                code: INVALID_REQUEST_ERROR_CODE,
                message: "codex account authentication required to read rate limits".to_string(),
                data: None,
            });
        };

        if !auth.is_chatgpt_auth() {
            return Err(JSONRPCErrorError {
                code: INVALID_REQUEST_ERROR_CODE,
                message: "chatgpt authentication required to read rate limits".to_string(),
                data: None,
            });
        }

        let client = BackendClient::from_auth(self.config.chatgpt_base_url.clone(), &auth)
            .map_err(|err| JSONRPCErrorError {
                code: INTERNAL_ERROR_CODE,
                message: format!("failed to construct backend client: {err}"),
                data: None,
            })?;

        let snapshots = client
            .get_rate_limits_many()
            .await
            .map_err(|err| JSONRPCErrorError {
                code: INTERNAL_ERROR_CODE,
                message: format!("failed to fetch codex rate limits: {err}"),
                data: None,
            })?;
        if snapshots.is_empty() {
            return Err(JSONRPCErrorError {
                code: INTERNAL_ERROR_CODE,
                message: "failed to fetch codex rate limits: no snapshots returned".to_string(),
                data: None,
            });
        }

        let rate_limits: HashMap<String, CoreRateLimitSnapshot> = snapshots
            .iter()
            .cloned()
            .map(|snapshot| {
                let limit_id = snapshot
                    .limit_id
                    .clone()
                    .unwrap_or_else(|| "codex".to_string());
                (limit_id, snapshot)
            })
            .collect();

        Ok(rate_limits)
    }
}

fn replace_cloud_config_bundle_loader(
    cloud_requirements: &RwLock<CloudConfigBundleLoader>,
    auth_manager: Arc<AuthManager>,
    chatgpt_base_url: String,
    praxis_home: PathBuf,
) {
    let loader = cloud_config_bundle_loader(auth_manager, chatgpt_base_url, praxis_home);
    if let Ok(mut guard) = cloud_requirements.write() {
        *guard = loader;
    } else {
        warn!("failed to update cloud requirements loader");
    }
}

async fn sync_default_client_residency_requirement(
    cli_overrides: &[(String, TomlValue)],
    cloud_requirements: &RwLock<CloudConfigBundleLoader>,
) {
    let loader = cloud_requirements
        .read()
        .map(|guard| guard.clone())
        .unwrap_or_default();
    match praxis_core::config::ConfigBuilder::default()
        .cli_overrides(cli_overrides.to_vec())
        .cloud_config_bundle(loader)
        .build()
        .await
    {
        Ok(config) => set_default_client_residency_requirement(config.enforce_residency.value()),
        Err(err) => warn!(
            error = %err,
            "failed to sync default client residency requirement after auth refresh"
        ),
    }
}
