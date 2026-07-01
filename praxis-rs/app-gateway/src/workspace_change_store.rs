use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;

use praxis_app_core::workspace_change::{
    WorkspaceChangeIndex, WorkspaceChangeSnapshot, WorkspaceHunkReviewAction,
    WorkspaceHunkReviewOutcome, WorkspaceHunkReviewResult, normalize_workspace_path,
    reject_hunk_in_file,
};
use praxis_app_gateway_protocol::WorkspaceChangeReviewHunkParams;
use tokio::sync::Mutex;

#[derive(Clone, Default)]
pub(crate) struct WorkspaceChangeStore {
    inner: Arc<Mutex<WorkspaceChangeStoreInner>>,
}

#[derive(Default)]
struct WorkspaceChangeStoreInner {
    indexes: HashMap<String, WorkspaceChangeIndex>,
}

impl WorkspaceChangeStore {
    pub(crate) async fn snapshot_or_empty(
        &self,
        root: PathBuf,
        thread_id: String,
    ) -> WorkspaceChangeSnapshot {
        let mut inner = self.inner.lock().await;
        inner
            .indexes
            .entry(thread_id.clone())
            .or_insert_with(|| empty_index(root, thread_id))
            .snapshot()
    }

    pub(crate) async fn update_from_diff(
        &self,
        root: PathBuf,
        thread_id: String,
        turn_id: Option<String>,
        diff: &str,
    ) -> WorkspaceChangeSnapshot {
        let source_revision = diff_source_revision(&root, &thread_id, turn_id.as_deref(), diff);
        let mut inner = self.inner.lock().await;
        let index = inner
            .indexes
            .entry(thread_id.clone())
            .or_insert_with(|| empty_index(root.clone(), thread_id.clone()));
        index.sync_from_diff(root, Some(thread_id), turn_id, diff, source_revision);
        index.snapshot()
    }

    pub(crate) async fn review_hunk(
        &self,
        params: WorkspaceChangeReviewHunkParams,
    ) -> (WorkspaceHunkReviewResult, WorkspaceChangeSnapshot) {
        let Some((root, hunk)) = self.review_target(&params).await else {
            let result = review_result(
                &params,
                WorkspaceHunkReviewOutcome::Failed,
                "The selected hunk is no longer present in the active workspace changes",
            );
            let snapshot = self
                .snapshot_or_empty(PathBuf::from("."), params.thread_id.clone())
                .await;
            return (result, snapshot);
        };

        let outcome = match params.action {
            WorkspaceHunkReviewAction::Accept => Ok(WorkspaceHunkReviewOutcome::Accepted),
            WorkspaceHunkReviewAction::Reject => {
                let normalized_path = normalize_workspace_path(&root, &params.path);
                reject_hunk_in_file(&normalized_path, &hunk)
                    .map(|()| WorkspaceHunkReviewOutcome::Rejected)
            }
        };

        let (outcome, message) = match outcome {
            Ok(WorkspaceHunkReviewOutcome::Accepted) => (
                WorkspaceHunkReviewOutcome::Accepted,
                format!("Accepted {}", params.path.display()),
            ),
            Ok(WorkspaceHunkReviewOutcome::Rejected) => (
                WorkspaceHunkReviewOutcome::Rejected,
                format!("Rejected {}", params.path.display()),
            ),
            Ok(WorkspaceHunkReviewOutcome::Conflict | WorkspaceHunkReviewOutcome::Failed) => {
                unreachable!("review hunk only produces accepted or rejected success outcomes")
            }
            Err(error) if error.contains("no longer matches") || error.contains("shorter") => {
                (WorkspaceHunkReviewOutcome::Conflict, error)
            }
            Err(error) => (WorkspaceHunkReviewOutcome::Failed, error),
        };

        let result = review_result(&params, outcome, message);
        let mut inner = self.inner.lock().await;
        let index = inner
            .indexes
            .entry(params.thread_id.clone())
            .or_insert_with(|| empty_index(root.clone(), params.thread_id.clone()));
        index.apply_review_result(&result);
        if matches!(outcome, WorkspaceHunkReviewOutcome::Rejected)
            && let Some(diff) = current_git_diff(&root)
        {
            let source_revision =
                diff_source_revision(&root, &params.thread_id, params.turn_id.as_deref(), &diff);
            index.sync_from_diff(
                root,
                Some(params.thread_id.clone()),
                params.turn_id.clone(),
                diff.as_str(),
                source_revision,
            );
        }
        (result, index.snapshot())
    }

    async fn review_target(
        &self,
        params: &WorkspaceChangeReviewHunkParams,
    ) -> Option<(
        PathBuf,
        praxis_app_core::workspace_change::WorkspaceChangeHunk,
    )> {
        let inner = self.inner.lock().await;
        let index = inner.indexes.get(&params.thread_id)?;
        if let Some(turn_id) = params.turn_id.as_deref()
            && index.turn_id.as_deref() != Some(turn_id)
        {
            return None;
        }
        let path = normalize_workspace_path(&index.root, &params.path);
        let file = index.files.get(&path)?;
        let hunk = file
            .hunks
            .iter()
            .find(|hunk| hunk.id == params.hunk_id && hunk.hash == params.hunk_hash)?;
        Some((index.root.clone(), hunk.clone()))
    }
}

fn empty_index(root: PathBuf, thread_id: String) -> WorkspaceChangeIndex {
    let mut index = WorkspaceChangeIndex::default();
    index.replace_from_snapshot(WorkspaceChangeSnapshot::empty(root, Some(thread_id)));
    index
}

fn review_result(
    params: &WorkspaceChangeReviewHunkParams,
    outcome: WorkspaceHunkReviewOutcome,
    message: impl Into<String>,
) -> WorkspaceHunkReviewResult {
    WorkspaceHunkReviewResult {
        thread_id: Some(params.thread_id.clone()),
        turn_id: params.turn_id.clone(),
        path: params.path.clone(),
        hunk_id: params.hunk_id.clone(),
        hunk_hash: params.hunk_hash,
        action: params.action,
        outcome,
        message: message.into(),
    }
}

fn diff_source_revision(root: &PathBuf, thread_id: &str, turn_id: Option<&str>, diff: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    root.hash(&mut hasher);
    thread_id.hash(&mut hasher);
    turn_id.hash(&mut hasher);
    diff.hash(&mut hasher);
    hasher.finish()
}

fn current_git_diff(root: &PathBuf) -> Option<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .arg("diff")
        .arg("--no-ext-diff")
        .arg("--")
        .arg(".")
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}
