use super::*;
use crate::json_rpc_error::internal_error;
use crate::json_rpc_error::invalid_request;
use praxis_thread_share::PublishMode;
use praxis_thread_share::PublishRequest;
use std::path::PathBuf;

impl PraxisMessageProcessor {
    pub(crate) async fn thread_share(
        &mut self,
        request_id: ConnectionRequestId,
        params: ThreadShareParams,
    ) {
        let team = params.team.trim().to_owned();
        if team.is_empty() || team.chars().count() > 64 || team.chars().any(char::is_control) {
            self.outgoing
                .send_error(
                    request_id,
                    invalid_request("team must contain 1-64 visible characters"),
                )
                .await;
            return;
        }
        let Some((thread_id, rollout_path)) = self
            .ensure_thread_rollout_for_request(
                &params.thread_id,
                ThreadRolloutScope::Any,
                &request_id,
            )
            .await
        else {
            return;
        };
        let thread = match self
            .load_thread_for_projection(thread_id, false, None)
            .await
        {
            Ok(Some(thread)) => thread,
            Ok(None) => {
                self.outgoing
                    .send_error(
                        request_id,
                        invalid_request(format!("thread not found: {thread_id}")),
                    )
                    .await;
                return;
            }
            Err(error) => {
                self.outgoing.send_error(request_id, error).await;
                return;
            }
        };
        let repository_path = std::env::var_os("PRAXIS_THREAD_SHARE_REPOSITORY")
            .map(PathBuf::from)
            .or_else(|| praxis_thread_share::discover_repository(&thread.cwd));
        let Some(repository_path) = repository_path else {
            self.outgoing
                .send_error(
                    request_id,
                    invalid_request(
                        "no praxis-threads checkout found for this project; set PRAXIS_THREAD_SHARE_REPOSITORY",
                    ),
                )
                .await;
            return;
        };
        let thread_id = thread_id.to_string();
        let publish_thread_id = thread_id.clone();
        let outcome = tokio::task::spawn_blocking(move || {
            praxis_thread_share::publish_thread(PublishRequest {
                rollout_path: &rollout_path,
                thread_id: &publish_thread_id,
                repository_path: &repository_path,
                team: &team,
                mode: PublishMode::Push,
            })
        })
        .await;
        let outcome = match outcome {
            Ok(Ok(outcome)) => outcome,
            Ok(Err(error)) => {
                self.outgoing
                    .send_error(
                        request_id,
                        internal_error(format!("failed to share thread: {error:#}")),
                    )
                    .await;
                return;
            }
            Err(error) => {
                self.outgoing
                    .send_error(
                        request_id,
                        internal_error(format!("thread share task failed: {error}")),
                    )
                    .await;
                return;
            }
        };
        self.outgoing
            .send_response(
                request_id,
                ThreadShareResponse {
                    thread_id,
                    project: outcome.project,
                    team: outcome.team,
                    message_count: outcome.message_count as u64,
                    redaction_count: outcome.redaction_count as u64,
                    commit: outcome.commit,
                    web_url: outcome.web_url,
                },
            )
            .await;
    }
}
