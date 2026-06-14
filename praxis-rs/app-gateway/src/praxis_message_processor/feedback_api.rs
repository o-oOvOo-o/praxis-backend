use std::path::PathBuf;

use praxis_app_gateway_protocol::FeedbackUploadParams;
use praxis_app_gateway_protocol::FeedbackUploadResponse;
use praxis_app_gateway_protocol::JSONRPCErrorError;
use praxis_protocol::ThreadId;
use praxis_rollout::state_db::get_state_db;
use tracing::warn;

use super::PraxisMessageProcessor;
use crate::error_code::INTERNAL_ERROR_CODE;
use crate::error_code::INVALID_REQUEST_ERROR_CODE;
use crate::outgoing_message::ConnectionRequestId;

impl PraxisMessageProcessor {
    pub(super) async fn upload_feedback(
        &self,
        request_id: ConnectionRequestId,
        params: FeedbackUploadParams,
    ) {
        if !self.config.feedback_enabled {
            let error = JSONRPCErrorError {
                code: INVALID_REQUEST_ERROR_CODE,
                message: "sending feedback is disabled by configuration".to_string(),
                data: None,
            };
            self.outgoing.send_error(request_id, error).await;
            return;
        }

        let FeedbackUploadParams {
            classification,
            reason,
            thread_id,
            include_logs,
            extra_log_files,
        } = params;

        let conversation_id = match thread_id.as_deref() {
            Some(thread_id) => match self.parse_thread_id(thread_id) {
                Ok(conversation_id) => Some(conversation_id),
                Err(error) => {
                    self.outgoing.send_error(request_id, error).await;
                    return;
                }
            },
            None => None,
        };

        if let Some(chatgpt_user_id) = self
            .auth_manager
            .auth_cached()
            .and_then(|auth| auth.get_chatgpt_user_id())
        {
            tracing::info!(target: "feedback_tags", chatgpt_user_id);
        }
        let snapshot = self.feedback.snapshot(conversation_id);
        let thread_id = snapshot.thread_id.clone();
        let sqlite_feedback_logs = if include_logs {
            if let Some(log_db) = self.log_db.as_ref() {
                log_db.flush().await;
            }
            let state_db_ctx = get_state_db(&self.config).await;
            match (state_db_ctx.as_ref(), conversation_id) {
                (Some(state_db_ctx), Some(conversation_id)) => {
                    let thread_id_text = conversation_id.to_string();
                    match state_db_ctx.query_feedback_logs(&thread_id_text).await {
                        Ok(logs) if logs.is_empty() => None,
                        Ok(logs) => Some(logs),
                        Err(err) => {
                            warn!(
                                "failed to query feedback logs from sqlite for thread_id={thread_id_text}: {err}"
                            );
                            None
                        }
                    }
                }
                _ => None,
            }
        } else {
            None
        };

        let validated_rollout_path = if include_logs {
            match conversation_id {
                Some(conv_id) => self.resolve_feedback_rollout_path(conv_id).await,
                None => None,
            }
        } else {
            None
        };
        let mut attachment_paths = validated_rollout_path.into_iter().collect::<Vec<_>>();
        if let Some(extra_log_files) = extra_log_files {
            attachment_paths.extend(extra_log_files);
        }

        let session_source = self.thread_manager.session_source();

        let upload_result = tokio::task::spawn_blocking(move || {
            snapshot.upload_feedback(
                &classification,
                reason.as_deref(),
                include_logs,
                &attachment_paths,
                Some(session_source),
                sqlite_feedback_logs,
            )
        })
        .await;

        let upload_result = match upload_result {
            Ok(result) => result,
            Err(join_err) => {
                let error = JSONRPCErrorError {
                    code: INTERNAL_ERROR_CODE,
                    message: format!("failed to upload feedback: {join_err}"),
                    data: None,
                };
                self.outgoing.send_error(request_id, error).await;
                return;
            }
        };

        match upload_result {
            Ok(()) => {
                let response = FeedbackUploadResponse { thread_id };
                self.outgoing.send_response(request_id, response).await;
            }
            Err(err) => {
                let error = JSONRPCErrorError {
                    code: INTERNAL_ERROR_CODE,
                    message: format!("failed to upload feedback: {err}"),
                    data: None,
                };
                self.outgoing.send_error(request_id, error).await;
            }
        }
    }

    async fn resolve_feedback_rollout_path(&self, conversation_id: ThreadId) -> Option<PathBuf> {
        match self.thread_manager.get_thread(conversation_id).await {
            Ok(conv) => conv.rollout_path(),
            Err(_) => None,
        }
    }
}
