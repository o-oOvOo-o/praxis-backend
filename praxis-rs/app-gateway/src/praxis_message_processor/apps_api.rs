use std::sync::Arc;
use std::time::Duration;

use praxis_app_gateway_protocol::AppInfo;
use praxis_app_gateway_protocol::AppsListParams;
use praxis_app_gateway_protocol::AppsListResponse;
use praxis_app_gateway_protocol::JSONRPCErrorError;
use praxis_chatgpt::connectors;
use praxis_core::config::Config;
use praxis_features::Feature;

use super::PraxisMessageProcessor;
use super::apps_list_helpers;
use crate::error_code::INTERNAL_ERROR_CODE;
use crate::error_code::INVALID_REQUEST_ERROR_CODE;
use crate::outgoing_message::ConnectionRequestId;
use crate::outgoing_message::OutgoingMessageSender;

const APP_LIST_LOAD_TIMEOUT: Duration = Duration::from_secs(90);

enum AppListLoadResult {
    Accessible(Result<Vec<AppInfo>, String>),
    Directory(Result<Vec<AppInfo>, String>),
}

impl PraxisMessageProcessor {
    pub(super) async fn apps_list(&self, request_id: ConnectionRequestId, params: AppsListParams) {
        let mut config = match self.load_latest_config(/*fallback_cwd*/ None).await {
            Ok(config) => config,
            Err(error) => {
                self.outgoing.send_error(request_id, error).await;
                return;
            }
        };

        if let Some(thread_id) = params.thread_id.as_deref() {
            let Some((_, thread)) = self.ensure_thread_for_request(thread_id, &request_id).await
            else {
                return;
            };
            let _ = config
                .features
                .set_enabled(Feature::Apps, thread.enabled(Feature::Apps));
        }

        if !config.features.apps_enabled(Some(&self.auth_manager)).await {
            self.outgoing
                .send_response(
                    request_id,
                    AppsListResponse {
                        data: Vec::new(),
                        next_cursor: None,
                    },
                )
                .await;
            return;
        }

        let outgoing = Arc::clone(&self.outgoing);
        tokio::spawn(async move {
            Self::apps_list_task(outgoing, request_id, params, config).await;
        });
    }

    async fn apps_list_task(
        outgoing: Arc<OutgoingMessageSender>,
        request_id: ConnectionRequestId,
        params: AppsListParams,
        config: Config,
    ) {
        let AppsListParams {
            cursor,
            limit,
            thread_id: _,
            force_refetch,
        } = params;
        let start = match cursor {
            Some(cursor) => match cursor.parse::<usize>() {
                Ok(idx) => idx,
                Err(_) => {
                    let error = JSONRPCErrorError {
                        code: INVALID_REQUEST_ERROR_CODE,
                        message: format!("invalid cursor: {cursor}"),
                        data: None,
                    };
                    outgoing.send_error(request_id, error).await;
                    return;
                }
            },
            None => 0,
        };

        let (mut accessible_connectors, mut all_connectors) = tokio::join!(
            connectors::list_cached_accessible_connectors_from_mcp_tools(&config),
            connectors::list_cached_all_connectors(&config)
        );
        let cached_all_connectors = all_connectors.clone();

        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

        let accessible_config = config.clone();
        let accessible_tx = tx.clone();
        tokio::spawn(async move {
            let result = connectors::list_accessible_connectors_from_mcp_tools_with_options(
                &accessible_config,
                force_refetch,
            )
            .await
            .map_err(|err| format!("failed to load accessible apps: {err}"));
            let _ = accessible_tx.send(AppListLoadResult::Accessible(result));
        });

        let all_config = config.clone();
        tokio::spawn(async move {
            let result = connectors::list_all_connectors_with_options(&all_config, force_refetch)
                .await
                .map_err(|err| format!("failed to list apps: {err}"));
            let _ = tx.send(AppListLoadResult::Directory(result));
        });

        let app_list_deadline = tokio::time::Instant::now() + APP_LIST_LOAD_TIMEOUT;
        let mut accessible_loaded = false;
        let mut all_loaded = false;
        let mut last_notified_apps = None;

        if accessible_connectors.is_some() || all_connectors.is_some() {
            let merged = connectors::with_app_enabled_state(
                apps_list_helpers::merge_loaded_apps(
                    all_connectors.as_deref(),
                    accessible_connectors.as_deref(),
                ),
                &config,
            );
            if apps_list_helpers::should_send_app_list_updated_notification(
                merged.as_slice(),
                accessible_loaded,
                all_loaded,
            ) {
                apps_list_helpers::send_app_list_updated_notification(&outgoing, merged.clone())
                    .await;
                last_notified_apps = Some(merged);
            }
        }

        loop {
            let result = match tokio::time::timeout_at(app_list_deadline, rx.recv()).await {
                Ok(Some(result)) => result,
                Ok(None) => {
                    let error = JSONRPCErrorError {
                        code: INTERNAL_ERROR_CODE,
                        message: "failed to load app lists".to_string(),
                        data: None,
                    };
                    outgoing.send_error(request_id, error).await;
                    return;
                }
                Err(_) => {
                    let timeout_seconds = APP_LIST_LOAD_TIMEOUT.as_secs();
                    let error = JSONRPCErrorError {
                        code: INTERNAL_ERROR_CODE,
                        message: format!(
                            "timed out waiting for app lists after {timeout_seconds} seconds"
                        ),
                        data: None,
                    };
                    outgoing.send_error(request_id, error).await;
                    return;
                }
            };

            match result {
                AppListLoadResult::Accessible(Ok(connectors)) => {
                    accessible_connectors = Some(connectors);
                    accessible_loaded = true;
                }
                AppListLoadResult::Accessible(Err(err)) => {
                    let error = JSONRPCErrorError {
                        code: INTERNAL_ERROR_CODE,
                        message: err,
                        data: None,
                    };
                    outgoing.send_error(request_id, error).await;
                    return;
                }
                AppListLoadResult::Directory(Ok(connectors)) => {
                    all_connectors = Some(connectors);
                    all_loaded = true;
                }
                AppListLoadResult::Directory(Err(err)) => {
                    let error = JSONRPCErrorError {
                        code: INTERNAL_ERROR_CODE,
                        message: err,
                        data: None,
                    };
                    outgoing.send_error(request_id, error).await;
                    return;
                }
            }

            let showing_interim_force_refetch = force_refetch && !(accessible_loaded && all_loaded);
            let all_connectors_for_update =
                if showing_interim_force_refetch && cached_all_connectors.is_some() {
                    cached_all_connectors.as_deref()
                } else {
                    all_connectors.as_deref()
                };
            let accessible_connectors_for_update =
                if showing_interim_force_refetch && !accessible_loaded {
                    None
                } else {
                    accessible_connectors.as_deref()
                };
            let merged = connectors::with_app_enabled_state(
                apps_list_helpers::merge_loaded_apps(
                    all_connectors_for_update,
                    accessible_connectors_for_update,
                ),
                &config,
            );
            if apps_list_helpers::should_send_app_list_updated_notification(
                merged.as_slice(),
                accessible_loaded,
                all_loaded,
            ) && last_notified_apps.as_ref() != Some(&merged)
            {
                apps_list_helpers::send_app_list_updated_notification(&outgoing, merged.clone())
                    .await;
                last_notified_apps = Some(merged.clone());
            }

            if accessible_loaded && all_loaded {
                match apps_list_helpers::paginate_apps(merged.as_slice(), start, limit) {
                    Ok(response) => {
                        outgoing.send_response(request_id, response).await;
                        return;
                    }
                    Err(error) => {
                        outgoing.send_error(request_id, error).await;
                        return;
                    }
                }
            }
        }
    }
}
