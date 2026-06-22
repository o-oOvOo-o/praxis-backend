use super::thread_rollout_locator::find_thread_rollout_path;
use super::*;

impl PraxisMessageProcessor {
    pub(crate) fn clear_plugin_related_caches(&self) {
        self.thread_manager.plugins_manager().clear_cache();
        self.thread_manager.skills_manager().clear_cache();
    }

    pub(crate) async fn maybe_start_plugin_startup_tasks_for_latest_config(&self) {
        match self.load_latest_config(/*fallback_cwd*/ None).await {
            Ok(config) => self
                .thread_manager
                .plugins_manager()
                .maybe_start_plugin_startup_tasks_for_config(
                    &config,
                    self.thread_manager.auth_manager(),
                ),
            Err(err) => warn!("failed to load latest config for plugin startup tasks: {err:?}"),
        }
    }

    pub(crate) async fn load_thread(
        &self,
        thread_id: &str,
    ) -> Result<(ThreadId, Arc<PraxisThread>), JSONRPCErrorError> {
        // Resolve the core conversation handle from a thread id string.
        let thread_id = self.parse_thread_id(thread_id)?;

        let thread = self
            .thread_manager
            .get_thread(thread_id)
            .await
            .map_err(|_| {
                crate::json_rpc_error::invalid_request(format!("thread not found: {thread_id}"))
            })?;

        Ok((thread_id, thread))
    }

    pub(crate) fn parse_thread_id(&self, thread_id: &str) -> Result<ThreadId, JSONRPCErrorError> {
        ThreadId::from_string(thread_id).map_err(|err| {
            crate::json_rpc_error::invalid_request(format!("invalid thread id: {err}"))
        })
    }

    pub(crate) async fn ensure_thread_id_for_request(
        &self,
        thread_id: &str,
        request_id: &ConnectionRequestId,
    ) -> Option<ThreadId> {
        match self.parse_thread_id(thread_id) {
            Ok(id) => Some(id),
            Err(error) => {
                self.outgoing.send_error(request_id.clone(), error).await;
                None
            }
        }
    }

    async fn ensure_thread_rollout_path_for_request(
        &self,
        thread_id: ThreadId,
        scope: ThreadRolloutScope,
        request_id: &ConnectionRequestId,
    ) -> Option<PathBuf> {
        match find_thread_rollout_path(&self.config, thread_id, scope).await {
            Ok(path) => Some(path),
            Err(error) => {
                self.outgoing.send_error(request_id.clone(), error).await;
                None
            }
        }
    }

    pub(super) async fn ensure_thread_rollout_for_request(
        &self,
        thread_id: &str,
        scope: ThreadRolloutScope,
        request_id: &ConnectionRequestId,
    ) -> Option<(ThreadId, PathBuf)> {
        let thread_id = self
            .ensure_thread_id_for_request(thread_id, request_id)
            .await?;
        let rollout_path = self
            .ensure_thread_rollout_path_for_request(thread_id, scope, request_id)
            .await?;
        Some((thread_id, rollout_path))
    }

    /// Load a thread by id, sending the error to the client on failure.
    /// Returns None when the thread could not be loaded (error already sent).
    pub(crate) async fn ensure_thread_for_request(
        &self,
        thread_id: &str,
        request_id: &ConnectionRequestId,
    ) -> Option<(ThreadId, Arc<PraxisThread>)> {
        match self.load_thread(thread_id).await {
            Ok(v) => Some(v),
            Err(error) => {
                self.outgoing.send_error(request_id.clone(), error).await;
                None
            }
        }
    }

    pub(crate) async fn load_latest_config(
        &self,
        fallback_cwd: Option<PathBuf>,
    ) -> Result<Config, JSONRPCErrorError> {
        let cloud_requirements = self.current_cloud_requirements();
        let mut config = praxis_core::config::ConfigBuilder::default()
            .cli_overrides(self.current_cli_overrides())
            .fallback_cwd(fallback_cwd)
            .cloud_config_bundle(cloud_requirements)
            .build()
            .await
            .map_err(|err| {
                crate::json_rpc_error::internal_error(format!("failed to reload config: {err}"))
            })?;
        apply_runtime_feature_enablement(&mut config, &self.current_runtime_feature_enablement());
        config.praxis_self_exe = self.arg0_paths.praxis_self_exe.clone();
        config.praxis_linux_sandbox_exe = self.arg0_paths.praxis_linux_sandbox_exe.clone();
        config.main_execve_wrapper_exe = self.arg0_paths.main_execve_wrapper_exe.clone();
        Ok(config)
    }

    pub(crate) fn current_cloud_requirements(&self) -> CloudConfigBundleLoader {
        self.cloud_requirements
            .read()
            .map(|guard| guard.clone())
            .unwrap_or_default()
    }

    pub(crate) fn current_cli_overrides(&self) -> Vec<(String, TomlValue)> {
        self.cli_overrides
            .read()
            .map(|guard| guard.clone())
            .unwrap_or_default()
    }

    pub(crate) fn current_runtime_feature_enablement(&self) -> BTreeMap<String, bool> {
        self.runtime_feature_enablement
            .read()
            .map(|guard| guard.clone())
            .unwrap_or_default()
    }

    pub(crate) async fn drain_background_tasks(&self) {
        self.background_tasks.close();
        if tokio::time::timeout(Duration::from_secs(10), self.background_tasks.wait())
            .await
            .is_err()
        {
            warn!("timed out waiting for background tasks to shut down; proceeding");
        }
    }

    pub(crate) async fn cancel_active_login(&self) {
        let mut guard = self.active_login.lock().await;
        if let Some(active_login) = guard.take() {
            drop(active_login);
        }
    }

    pub(crate) async fn clear_all_thread_listeners(&self) {
        self.thread_state_manager.clear_all_listeners().await;
    }

    pub(crate) async fn shutdown_threads(&self) {
        let report = self
            .thread_manager
            .shutdown_all_threads_bounded(Duration::from_secs(10))
            .await;
        for thread_id in report.submit_failed {
            warn!("failed to submit Shutdown to thread {thread_id}");
        }
        for thread_id in report.timed_out {
            warn!("timed out waiting for thread {thread_id} to shut down");
        }
    }

    pub(crate) async fn request_trace_context(
        &self,
        request_id: &ConnectionRequestId,
    ) -> Option<praxis_protocol::protocol::W3cTraceContext> {
        self.outgoing.request_trace_context(request_id).await
    }

    pub(crate) async fn submit_core_op(
        &self,
        request_id: &ConnectionRequestId,
        thread: &PraxisThread,
        op: Op,
    ) -> PraxisResult<String> {
        thread
            .submit_with_trace(op, self.request_trace_context(request_id).await)
            .await
    }

    pub(crate) async fn send_invalid_request_error(
        &self,
        request_id: ConnectionRequestId,
        message: String,
    ) {
        let error = crate::json_rpc_error::invalid_request(message);
        self.outgoing.send_error(request_id, error).await;
    }

    pub(crate) async fn send_internal_error(
        &self,
        request_id: ConnectionRequestId,
        message: String,
    ) {
        let error = crate::json_rpc_error::internal_error(message);
        self.outgoing.send_error(request_id, error).await;
    }

    pub(crate) async fn send_result_response<T: serde::Serialize>(
        &self,
        request_id: ConnectionRequestId,
        result: std::result::Result<T, JSONRPCErrorError>,
    ) {
        match result {
            Ok(response) => self.outgoing.send_response(request_id, response).await,
            Err(error) => self.outgoing.send_error(request_id, error).await,
        }
    }

    pub(crate) async fn send_marketplace_error(
        &self,
        request_id: ConnectionRequestId,
        err: MarketplaceError,
        action: &str,
    ) {
        match err {
            MarketplaceError::MarketplaceNotFound { .. } => {
                self.send_invalid_request_error(request_id, err.to_string())
                    .await;
            }
            MarketplaceError::Io { .. } => {
                self.send_internal_error(request_id, format!("failed to {action}: {err}"))
                    .await;
            }
            MarketplaceError::InvalidMarketplaceFile { .. }
            | MarketplaceError::PluginNotFound { .. }
            | MarketplaceError::PluginNotAvailable { .. }
            | MarketplaceError::PluginsDisabled
            | MarketplaceError::InvalidPlugin(_) => {
                self.send_invalid_request_error(request_id, err.to_string())
                    .await;
            }
        }
    }
}
