use std::sync::Arc;

use praxis_app_gateway_protocol::CollaborationModeListParams;
use praxis_app_gateway_protocol::CollaborationModeListResponse;
use praxis_app_gateway_protocol::ExperimentalFeature as ApiExperimentalFeature;
use praxis_app_gateway_protocol::ExperimentalFeatureListParams;
use praxis_app_gateway_protocol::ExperimentalFeatureListResponse;
use praxis_app_gateway_protocol::ExperimentalFeatureStage as ApiExperimentalFeatureStage;
use praxis_app_gateway_protocol::JSONRPCErrorError;
use praxis_app_gateway_protocol::MockExperimentalMethodParams;
use praxis_app_gateway_protocol::MockExperimentalMethodResponse;
use praxis_app_gateway_protocol::ModelListParams;
use praxis_app_gateway_protocol::ModelListResponse;
use praxis_core::ThreadManager;
use praxis_core::config::Config;
use praxis_features::FEATURES;
use praxis_features::Stage;

use super::PraxisMessageProcessor;
use crate::error_code::INVALID_REQUEST_ERROR_CODE;
use crate::models::supported_models;
use crate::outgoing_message::ConnectionRequestId;
use crate::outgoing_message::OutgoingMessageSender;

impl PraxisMessageProcessor {
    pub(super) async fn model_list(
        &self,
        request_id: ConnectionRequestId,
        params: ModelListParams,
    ) {
        let outgoing = self.outgoing.clone();
        let thread_manager = self.thread_manager.clone();
        let config = match self.load_latest_config(/*fallback_cwd*/ None).await {
            Ok(config) => config,
            Err(error) => {
                self.outgoing.send_error(request_id, error).await;
                return;
            }
        };

        tokio::spawn(async move {
            Self::model_list_task(outgoing, thread_manager, config, request_id, params).await;
        });
    }

    async fn model_list_task(
        outgoing: Arc<OutgoingMessageSender>,
        thread_manager: Arc<ThreadManager>,
        config: Config,
        request_id: ConnectionRequestId,
        params: ModelListParams,
    ) {
        let ModelListParams {
            limit,
            cursor,
            include_hidden,
        } = params;
        let models =
            supported_models(thread_manager, &config, include_hidden.unwrap_or(false)).await;
        let total = models.len();

        if total == 0 {
            let response = ModelListResponse {
                data: Vec::new(),
                next_cursor: None,
            };
            outgoing.send_response(request_id, response).await;
            return;
        }

        let effective_limit = limit.unwrap_or(total as u32).max(1) as usize;
        let effective_limit = effective_limit.min(total);
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

        if start > total {
            let error = JSONRPCErrorError {
                code: INVALID_REQUEST_ERROR_CODE,
                message: format!("cursor {start} exceeds total models {total}"),
                data: None,
            };
            outgoing.send_error(request_id, error).await;
            return;
        }

        let end = start.saturating_add(effective_limit).min(total);
        let items = models[start..end].to_vec();
        let next_cursor = if end < total {
            Some(end.to_string())
        } else {
            None
        };
        let response = ModelListResponse {
            data: items,
            next_cursor,
        };
        outgoing.send_response(request_id, response).await;
    }

    pub(super) async fn collaboration_mode_list(
        &self,
        request_id: ConnectionRequestId,
        params: CollaborationModeListParams,
    ) {
        let outgoing = self.outgoing.clone();
        let thread_manager = self.thread_manager.clone();

        tokio::spawn(async move {
            Self::collaboration_mode_list_task(outgoing, thread_manager, request_id, params).await;
        });
    }

    async fn collaboration_mode_list_task(
        outgoing: Arc<OutgoingMessageSender>,
        thread_manager: Arc<ThreadManager>,
        request_id: ConnectionRequestId,
        params: CollaborationModeListParams,
    ) {
        let CollaborationModeListParams {} = params;
        let items = thread_manager
            .list_collaboration_modes()
            .into_iter()
            .map(Into::into)
            .collect();
        let response = CollaborationModeListResponse { data: items };
        outgoing.send_response(request_id, response).await;
    }

    pub(super) async fn experimental_feature_list(
        &self,
        request_id: ConnectionRequestId,
        params: ExperimentalFeatureListParams,
    ) {
        let ExperimentalFeatureListParams { cursor, limit } = params;
        let config = match self.load_latest_config(/*fallback_cwd*/ None).await {
            Ok(config) => config,
            Err(error) => {
                self.outgoing.send_error(request_id, error).await;
                return;
            }
        };

        let data = FEATURES
            .iter()
            .map(|spec| {
                let (stage, display_name, description, announcement) = match spec.stage {
                    Stage::Experimental {
                        name,
                        menu_description,
                        announcement,
                    } => (
                        ApiExperimentalFeatureStage::Beta,
                        Some(name.to_string()),
                        Some(menu_description.to_string()),
                        Some(announcement.to_string()),
                    ),
                    Stage::UnderDevelopment => (
                        ApiExperimentalFeatureStage::UnderDevelopment,
                        None,
                        None,
                        None,
                    ),
                    Stage::Stable => (ApiExperimentalFeatureStage::Stable, None, None, None),
                    Stage::Deprecated => {
                        (ApiExperimentalFeatureStage::Deprecated, None, None, None)
                    }
                    Stage::Removed => (ApiExperimentalFeatureStage::Removed, None, None, None),
                };

                ApiExperimentalFeature {
                    name: spec.key.to_string(),
                    stage,
                    display_name,
                    description,
                    announcement,
                    enabled: config.features.enabled(spec.id),
                    default_enabled: spec.default_enabled,
                }
            })
            .collect::<Vec<_>>();

        let total = data.len();
        if total == 0 {
            self.outgoing
                .send_response(
                    request_id,
                    ExperimentalFeatureListResponse {
                        data: Vec::new(),
                        next_cursor: None,
                    },
                )
                .await;
            return;
        }

        let effective_limit = limit.unwrap_or(total as u32).max(1) as usize;
        let effective_limit = effective_limit.min(total);
        let start = match cursor {
            Some(cursor) => match cursor.parse::<usize>() {
                Ok(idx) => idx,
                Err(_) => {
                    self.send_invalid_request_error(
                        request_id,
                        format!("invalid cursor: {cursor}"),
                    )
                    .await;
                    return;
                }
            },
            None => 0,
        };

        if start > total {
            self.send_invalid_request_error(
                request_id,
                format!("cursor {start} exceeds total feature flags {total}"),
            )
            .await;
            return;
        }

        let end = start.saturating_add(effective_limit).min(total);
        let data = data[start..end].to_vec();
        let next_cursor = if end < total {
            Some(end.to_string())
        } else {
            None
        };

        self.outgoing
            .send_response(
                request_id,
                ExperimentalFeatureListResponse { data, next_cursor },
            )
            .await;
    }

    pub(super) async fn mock_experimental_method(
        &self,
        request_id: ConnectionRequestId,
        params: MockExperimentalMethodParams,
    ) {
        let MockExperimentalMethodParams { value } = params;
        let response = MockExperimentalMethodResponse { echoed: value };
        self.outgoing.send_response(request_id, response).await;
    }
}
