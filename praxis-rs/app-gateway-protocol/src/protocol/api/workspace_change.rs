use super::*;

pub use praxis_app_core::workspace_change::WorkspaceChangeDiffLine;
pub use praxis_app_core::workspace_change::WorkspaceChangeDiffLineKind;
pub use praxis_app_core::workspace_change::WorkspaceChangeFile;
pub use praxis_app_core::workspace_change::WorkspaceChangeHunk;
pub use praxis_app_core::workspace_change::WorkspaceChangeReviewState;
pub use praxis_app_core::workspace_change::WorkspaceChangeSnapshot;
pub use praxis_app_core::workspace_change::WorkspaceChangeStatus;
pub use praxis_app_core::workspace_change::WorkspaceChangeSummary;
pub use praxis_app_core::workspace_change::WorkspaceHunkReviewAction;
pub use praxis_app_core::workspace_change::WorkspaceHunkReviewOutcome;
pub use praxis_app_core::workspace_change::WorkspaceHunkReviewResult;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceChangeGetParams {
    pub thread_id: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceChangeGetResponse {
    pub snapshot: WorkspaceChangeSnapshot,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceChangeReviewHunkParams {
    pub thread_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional = nullable)]
    pub turn_id: Option<String>,
    pub path: PathBuf,
    pub hunk_id: String,
    pub hunk_hash: u64,
    pub action: WorkspaceHunkReviewAction,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceChangeReviewHunkResponse {
    pub result: WorkspaceHunkReviewResult,
    pub snapshot: WorkspaceChangeSnapshot,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceChangeUpdatedNotification {
    pub thread_id: String,
    pub snapshot: WorkspaceChangeSnapshot,
}
