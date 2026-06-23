use crate::cache::{CloudRequirementsCache, auth_identity};
use crate::constants::{
    CLOUD_REQUIREMENTS_AUTH_RECOVERY_FAILED_MESSAGE, CLOUD_REQUIREMENTS_CACHE_REFRESH_INTERVAL,
    CLOUD_REQUIREMENTS_LOAD_FAILED_MESSAGE, CLOUD_REQUIREMENTS_MAX_ATTEMPTS,
};
use crate::fetcher::{FetchAttemptError, RequirementsFetcher};
use crate::metrics::{emit_fetch_attempt_metric, emit_fetch_final_metric, emit_load_metric};
use crate::parsing::bundle_from_requirements_contents;
#[cfg(test)]
use crate::parsing::{
    cloud_bundle_error_to_requirements_error, requirements_from_bundle_option,
    requirements_parse_error,
};
#[cfg(test)]
use praxis_core::config_loader::CloudRequirementsLoadError;
use praxis_core::config_loader::{
    CloudConfigBundle, CloudConfigBundleLoadError, CloudConfigBundleLoadErrorCode,
};
use praxis_core::util::backoff;
use praxis_login::{AuthManager, OpenAiAccountAuth, RefreshTokenError};
use praxis_protocol::account::PlanType;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::time::{sleep, timeout};

#[derive(Clone)]
pub(crate) struct CloudRequirementsService {
    auth_manager: Arc<AuthManager>,
    fetcher: Arc<dyn RequirementsFetcher>,
    cache: CloudRequirementsCache,
    timeout: Duration,
}

impl CloudRequirementsService {
    pub(crate) fn new(
        auth_manager: Arc<AuthManager>,
        fetcher: Arc<dyn RequirementsFetcher>,
        praxis_home: std::path::PathBuf,
        timeout: Duration,
    ) -> Self {
        Self {
            auth_manager,
            fetcher,
            cache: CloudRequirementsCache::new(praxis_home),
            timeout,
        }
    }

    #[cfg(test)]
    pub(crate) async fn fetch_with_timeout(
        &self,
    ) -> Result<
        Option<praxis_core::config_loader::ConfigRequirementsToml>,
        CloudRequirementsLoadError,
    > {
        let bundle = self
            .fetch_bundle_with_timeout()
            .await
            .map_err(cloud_bundle_error_to_requirements_error)?;
        requirements_from_bundle_option(bundle).map_err(|err| {
            tracing::error!(error = %err, "Failed to parse cloud requirements bundle");
            requirements_parse_error()
        })
    }

    pub(crate) async fn fetch_bundle_with_timeout(
        &self,
    ) -> Result<Option<CloudConfigBundle>, CloudConfigBundleLoadError> {
        let _timer =
            praxis_otel::start_global_timer("praxis.cloud_requirements.fetch.duration_ms", &[]);
        let started_at = Instant::now();
        let fetch_result = timeout(self.timeout, self.fetch_bundle())
            .await
            .inspect_err(|_| {
                let message = format!(
                    "Timed out waiting for cloud requirements after {}s",
                    self.timeout.as_secs()
                );
                tracing::error!("{message}");
                emit_load_metric("startup", "error");
            })
            .map_err(|_| {
                CloudConfigBundleLoadError::new(
                    CloudConfigBundleLoadErrorCode::Timeout,
                    /*status_code*/ None,
                    format!(
                        "timed out waiting for cloud requirements after {}s",
                        self.timeout.as_secs()
                    ),
                )
            })?;

        let result = match fetch_result {
            Ok(result) => result,
            Err(err) => {
                emit_load_metric("startup", "error");
                return Err(err);
            }
        };

        match result.as_ref() {
            Some(requirements) => {
                tracing::info!(
                    elapsed_ms = started_at.elapsed().as_millis(),
                    requirements = ?requirements,
                    "Cloud requirements load completed"
                );
                emit_load_metric("startup", "success");
            }
            None => {
                tracing::info!(
                    elapsed_ms = started_at.elapsed().as_millis(),
                    "Cloud requirements load completed (none)"
                );
                emit_load_metric("startup", "success");
            }
        }

        Ok(result)
    }

    #[cfg(test)]
    pub(crate) async fn fetch(
        &self,
    ) -> Result<
        Option<praxis_core::config_loader::ConfigRequirementsToml>,
        CloudRequirementsLoadError,
    > {
        let bundle = self
            .fetch_bundle()
            .await
            .map_err(cloud_bundle_error_to_requirements_error)?;
        requirements_from_bundle_option(bundle).map_err(|err| {
            tracing::error!(error = %err, "Failed to parse cloud requirements bundle");
            requirements_parse_error()
        })
    }

    async fn fetch_bundle(&self) -> Result<Option<CloudConfigBundle>, CloudConfigBundleLoadError> {
        let Some(auth) = self.auth_manager.auth().await else {
            return Ok(None);
        };
        if !is_cloud_requirements_eligible(&auth) {
            return Ok(None);
        }
        let (chatgpt_user_id, account_id) = auth_identity(&auth);

        match self
            .cache
            .load(chatgpt_user_id.as_deref(), account_id.as_deref())
            .await
        {
            Ok(signed_payload) => {
                tracing::info!(
                    path = %self.cache.path().display(),
                    "Using cached cloud requirements"
                );
                match signed_payload.bundle() {
                    Ok(bundle) => return Ok(bundle),
                    Err(err) => {
                        tracing::warn!(
                            error = %err,
                            path = %self.cache.path().display(),
                            "Ignoring cached cloud requirements because cached bundle is invalid"
                        );
                    }
                }
            }
            Err(cache_load_status) => self.cache.log_load_status(&cache_load_status),
        }

        self.fetch_bundle_with_retries(auth, "startup").await
    }

    async fn fetch_bundle_with_retries(
        &self,
        mut auth: OpenAiAccountAuth,
        trigger: &'static str,
    ) -> Result<Option<CloudConfigBundle>, CloudConfigBundleLoadError> {
        let mut attempt = 1;
        let mut last_status_code: Option<u16> = None;
        let mut auth_recovery = self.auth_manager.unauthorized_recovery();

        while attempt <= CLOUD_REQUIREMENTS_MAX_ATTEMPTS {
            let contents = match self.fetcher.fetch_requirements(&auth).await {
                Ok(contents) => {
                    emit_fetch_attempt_metric(
                        trigger, attempt, "success", /*status_code*/ None,
                    );
                    contents
                }
                Err(FetchAttemptError::Retryable(status)) => {
                    let status_code = status.status_code();
                    last_status_code = status_code;
                    emit_fetch_attempt_metric(trigger, attempt, "error", status_code);
                    if attempt < CLOUD_REQUIREMENTS_MAX_ATTEMPTS {
                        tracing::warn!(
                            status = ?status,
                            attempt,
                            max_attempts = CLOUD_REQUIREMENTS_MAX_ATTEMPTS,
                            "Failed to fetch cloud requirements; retrying"
                        );
                        sleep(backoff(attempt as u64)).await;
                    }
                    attempt += 1;
                    continue;
                }
                Err(FetchAttemptError::Unauthorized {
                    status_code,
                    message,
                }) => {
                    last_status_code = status_code;
                    emit_fetch_attempt_metric(trigger, attempt, "unauthorized", status_code);
                    if auth_recovery.has_next() {
                        tracing::warn!(
                            attempt,
                            max_attempts = CLOUD_REQUIREMENTS_MAX_ATTEMPTS,
                            "Cloud requirements request was unauthorized; attempting auth recovery"
                        );
                        match auth_recovery.next().await {
                            Ok(_) => {
                                let Some(refreshed_auth) = self.auth_manager.auth().await else {
                                    tracing::error!(
                                        "Auth recovery succeeded but no auth is available for cloud requirements"
                                    );
                                    emit_fetch_final_metric(
                                        trigger,
                                        "error",
                                        "auth_recovery_missing_auth",
                                        attempt,
                                        status_code,
                                    );
                                    return Err(CloudConfigBundleLoadError::new(
                                        CloudConfigBundleLoadErrorCode::Auth,
                                        status_code,
                                        CLOUD_REQUIREMENTS_AUTH_RECOVERY_FAILED_MESSAGE,
                                    ));
                                };
                                auth = refreshed_auth;
                                continue;
                            }
                            Err(RefreshTokenError::Permanent(failed)) => {
                                tracing::warn!(
                                    error = %failed,
                                    "Failed to recover from unauthorized cloud requirements request"
                                );
                                emit_fetch_final_metric(
                                    trigger,
                                    "error",
                                    "auth_recovery_unrecoverable",
                                    attempt,
                                    status_code,
                                );
                                return Err(CloudConfigBundleLoadError::new(
                                    CloudConfigBundleLoadErrorCode::Auth,
                                    status_code,
                                    failed.message,
                                ));
                            }
                            Err(RefreshTokenError::Transient(recovery_err)) => {
                                if attempt < CLOUD_REQUIREMENTS_MAX_ATTEMPTS {
                                    tracing::warn!(
                                        error = %recovery_err,
                                        attempt,
                                        max_attempts = CLOUD_REQUIREMENTS_MAX_ATTEMPTS,
                                        "Failed to recover from unauthorized cloud requirements request; retrying"
                                    );
                                    sleep(backoff(attempt as u64)).await;
                                }
                                attempt += 1;
                                continue;
                            }
                        }
                    }

                    tracing::warn!(
                        error = %message,
                        "Cloud requirements request was unauthorized and no auth recovery is available"
                    );
                    emit_fetch_final_metric(
                        trigger,
                        "error",
                        "auth_recovery_unavailable",
                        attempt,
                        status_code,
                    );
                    return Err(CloudConfigBundleLoadError::new(
                        CloudConfigBundleLoadErrorCode::Auth,
                        status_code,
                        CLOUD_REQUIREMENTS_AUTH_RECOVERY_FAILED_MESSAGE,
                    ));
                }
            };

            let bundle = match bundle_from_requirements_contents(contents.clone()) {
                Ok(bundle) => bundle,
                Err(err) => {
                    tracing::error!(error = %err, "Failed to parse cloud requirements");
                    emit_fetch_final_metric(
                        trigger,
                        "error",
                        "parse_error",
                        attempt,
                        last_status_code,
                    );
                    return Err(CloudConfigBundleLoadError::new(
                        CloudConfigBundleLoadErrorCode::InvalidBundle,
                        /*status_code*/ None,
                        CLOUD_REQUIREMENTS_LOAD_FAILED_MESSAGE,
                    ));
                }
            };

            let (chatgpt_user_id, account_id) = auth_identity(&auth);
            if let Err(err) = self.cache.save(chatgpt_user_id, account_id, contents).await {
                tracing::warn!(error = %err, "Failed to write cloud requirements cache");
            }

            emit_fetch_final_metric(
                trigger, "success", "none", attempt, /*status_code*/ None,
            );
            return Ok(bundle);
        }

        emit_fetch_final_metric(
            trigger,
            "error",
            "request_retry_exhausted",
            CLOUD_REQUIREMENTS_MAX_ATTEMPTS,
            last_status_code,
        );
        tracing::error!(
            path = %self.cache.path().display(),
            "{CLOUD_REQUIREMENTS_LOAD_FAILED_MESSAGE}"
        );
        Err(CloudConfigBundleLoadError::new(
            CloudConfigBundleLoadErrorCode::RequestFailed,
            last_status_code,
            CLOUD_REQUIREMENTS_LOAD_FAILED_MESSAGE,
        ))
    }

    pub(crate) async fn refresh_cache_in_background(&self) {
        loop {
            sleep(CLOUD_REQUIREMENTS_CACHE_REFRESH_INTERVAL).await;
            match timeout(self.timeout, self.refresh_cache()).await {
                Ok(true) => {}
                Ok(false) => break,
                Err(_) => {
                    tracing::error!(
                        "Timed out refreshing cloud requirements cache from remote; keeping existing cache"
                    );
                    emit_load_metric("refresh", "error");
                }
            }
        }
    }

    async fn refresh_cache(&self) -> bool {
        let Some(auth) = self.auth_manager.auth().await else {
            return false;
        };
        if !is_cloud_requirements_eligible(&auth) {
            return false;
        }

        match self.fetch_bundle_with_retries(auth, "refresh").await {
            Ok(_) => emit_load_metric("refresh", "success"),
            Err(err) => {
                tracing::error!(
                    path = %self.cache.path().display(),
                    error = %err,
                    "Failed to refresh cloud requirements cache from remote"
                );
                emit_load_metric("refresh", "error");
            }
        }
        true
    }
}

fn is_cloud_requirements_eligible(auth: &OpenAiAccountAuth) -> bool {
    let Some(plan_type) = auth.account_plan_type() else {
        return false;
    };
    auth.is_chatgpt_auth()
        && (plan_type.is_business_like() || matches!(plan_type, PlanType::Enterprise))
}
