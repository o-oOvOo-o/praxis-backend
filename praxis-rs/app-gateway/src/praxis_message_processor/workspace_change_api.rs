use super::*;
use praxis_app_gateway_protocol::ServerNotification;
use praxis_app_gateway_protocol::WorkspaceChangeGetParams;
use praxis_app_gateway_protocol::WorkspaceChangeGetResponse;
use praxis_app_gateway_protocol::WorkspaceChangeReviewHunkParams;
use praxis_app_gateway_protocol::WorkspaceChangeReviewHunkResponse;
use praxis_app_gateway_protocol::WorkspaceChangeUpdatedNotification;

impl PraxisMessageProcessor {
    pub(crate) async fn workspace_change_get(
        &mut self,
        request_id: ConnectionRequestId,
        params: WorkspaceChangeGetParams,
    ) {
        let Some(thread_uuid) = self
            .ensure_thread_id_for_request(&params.thread_id, &request_id)
            .await
        else {
            return;
        };
        let Some(root) = self
            .workspace_change_thread_root(thread_uuid, &request_id)
            .await
        else {
            return;
        };
        let snapshot = self
            .workspace_change_store
            .snapshot_or_empty(root, params.thread_id)
            .await;
        self.outgoing
            .send_response(request_id, WorkspaceChangeGetResponse { snapshot })
            .await;
    }

    pub(crate) async fn workspace_change_review_hunk(
        &mut self,
        request_id: ConnectionRequestId,
        params: WorkspaceChangeReviewHunkParams,
    ) {
        let Some(_thread_uuid) = self
            .ensure_thread_id_for_request(&params.thread_id, &request_id)
            .await
        else {
            return;
        };
        let thread_id = params.thread_id.clone();
        let (result, snapshot) = self.workspace_change_store.review_hunk(params).await;
        self.outgoing
            .send_response(
                request_id,
                WorkspaceChangeReviewHunkResponse {
                    result,
                    snapshot: snapshot.clone(),
                },
            )
            .await;
        self.outgoing
            .send_server_notification(ServerNotification::WorkspaceChangeUpdated(
                WorkspaceChangeUpdatedNotification {
                    thread_id,
                    snapshot,
                },
            ))
            .await;
    }

    async fn workspace_change_thread_root(
        &self,
        thread_uuid: ThreadId,
        request_id: &ConnectionRequestId,
    ) -> Option<PathBuf> {
        match self.load_thread_for_projection(thread_uuid, false).await {
            Ok(Some(thread)) => Some(thread.cwd),
            Ok(None) => {
                self.send_invalid_request_error(
                    request_id.clone(),
                    format!("thread not loaded: {thread_uuid}"),
                )
                .await;
                None
            }
            Err(error) => {
                self.outgoing.send_error(request_id.clone(), error).await;
                None
            }
        }
    }
}
