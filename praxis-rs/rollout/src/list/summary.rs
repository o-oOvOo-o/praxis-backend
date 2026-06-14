use std::io;
use std::path::Path;
use std::path::PathBuf;

use praxis_protocol::ThreadId;
use praxis_protocol::protocol::RolloutItem;
use praxis_protocol::protocol::RolloutLine;
use praxis_protocol::protocol::SessionMetaLine;
use praxis_protocol::protocol::SessionSource;

use super::ThreadItem;

const HEAD_RECORD_LIMIT: usize = 10;
const USER_EVENT_SCAN_LIMIT: usize = 200;

#[derive(Default)]
pub(super) struct HeadSummary {
    pub(super) saw_session_meta: bool,
    pub(super) saw_user_message: bool,
    pub(super) fields: ThreadSummaryFields,
}

#[derive(Default)]
pub(super) struct ThreadSummaryFields {
    pub(super) thread_id: Option<ThreadId>,
    pub(super) first_user_message: Option<String>,
    pub(super) cwd: Option<PathBuf>,
    pub(super) git_branch: Option<String>,
    pub(super) git_sha: Option<String>,
    pub(super) git_origin_url: Option<String>,
    pub(super) source: Option<SessionSource>,
    pub(super) agent_base_name: Option<String>,
    pub(super) agent_title: Option<String>,
    pub(super) agent_display_name: Option<String>,
    pub(super) agent_role: Option<String>,
    pub(super) model_provider: Option<String>,
    pub(super) cli_version: Option<String>,
    pub(super) created_at: Option<String>,
    pub(super) updated_at: Option<String>,
}

impl ThreadSummaryFields {
    pub(super) fn into_thread_item(
        self,
        path: PathBuf,
        updated_at_fallback: Option<String>,
    ) -> ThreadItem {
        let ThreadSummaryFields {
            thread_id,
            first_user_message,
            cwd,
            git_branch,
            git_sha,
            git_origin_url,
            source,
            agent_base_name,
            agent_title,
            agent_display_name,
            agent_role,
            model_provider,
            cli_version,
            created_at,
            updated_at,
        } = self;
        let updated_at = updated_at
            .or(updated_at_fallback)
            .or_else(|| created_at.clone());
        ThreadItem {
            path,
            thread_id,
            first_user_message,
            cwd,
            git_branch,
            git_sha,
            git_origin_url,
            source,
            agent_base_name,
            agent_title,
            agent_display_name,
            agent_role,
            model_provider,
            cli_version,
            created_at,
            updated_at,
        }
    }
}

pub(super) async fn read_head_summary(path: &Path) -> io::Result<HeadSummary> {
    use tokio::io::AsyncBufReadExt;

    let file = tokio::fs::File::open(path).await?;
    let reader = tokio::io::BufReader::new(file);
    let mut lines = reader.lines();
    let mut summary = HeadSummary::default();
    let mut lines_scanned = 0usize;

    while lines_scanned < HEAD_RECORD_LIMIT
        || (summary.saw_session_meta
            && !summary.saw_user_message
            && lines_scanned < HEAD_RECORD_LIMIT + USER_EVENT_SCAN_LIMIT)
    {
        let line_opt = lines.next_line().await?;
        let Some(line) = line_opt else { break };
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        lines_scanned += 1;

        let parsed: Result<RolloutLine, _> = serde_json::from_str(trimmed);
        let Ok(rollout_line) = parsed else {
            continue;
        };

        let preview = praxis_state::thread_preview::rollout_item_preview(&rollout_line.item);
        match rollout_line.item {
            RolloutItem::SessionMeta(session_meta_line) => {
                if !summary.saw_session_meta {
                    summary.fields.source = Some(session_meta_line.meta.source.clone());
                    summary.fields.agent_base_name = session_meta_line.meta.agent_base_name.clone();
                    summary.fields.agent_title = session_meta_line.meta.agent_title.clone();
                    summary.fields.agent_display_name =
                        session_meta_line.meta.agent_display_name.clone();
                    summary.fields.agent_role = session_meta_line.meta.agent_role.clone();
                    summary.fields.model_provider = session_meta_line.meta.model_provider.clone();
                    summary.fields.thread_id = Some(session_meta_line.meta.id);
                    summary.fields.cwd = Some(session_meta_line.meta.cwd.clone());
                    summary.fields.git_branch = session_meta_line
                        .git
                        .as_ref()
                        .and_then(|git| git.branch.clone());
                    summary.fields.git_sha = session_meta_line
                        .git
                        .as_ref()
                        .and_then(|git| git.commit_hash.as_ref().map(|sha| sha.0.clone()));
                    summary.fields.git_origin_url = session_meta_line
                        .git
                        .as_ref()
                        .and_then(|git| git.repository_url.clone());
                    summary.fields.cli_version = Some(session_meta_line.meta.cli_version);
                    summary.fields.created_at = Some(session_meta_line.meta.timestamp.clone());
                    summary.saw_session_meta = true;
                }
            }
            RolloutItem::ResponseItem(_) => {
                summary.fields.created_at = summary
                    .fields
                    .created_at
                    .clone()
                    .or_else(|| Some(rollout_line.timestamp.clone()));
            }
            RolloutItem::TurnContext(_) => {
                // Not included in `head`; skip.
            }
            RolloutItem::Compacted(_) => {
                // Not included in `head`; skip.
            }
            RolloutItem::EventMsg(_) => {}
        }

        if let Some(preview) = preview {
            summary.saw_user_message = true;
            if summary.fields.first_user_message.is_none() {
                summary.fields.first_user_message = Some(preview.into_display_text());
            }
        }

        if summary.saw_session_meta && summary.saw_user_message {
            break;
        }
    }

    Ok(summary)
}

/// Read up to `HEAD_RECORD_LIMIT` records from the start of the rollout file at `path`.
/// This should be enough to produce a summary including the session meta line.
pub async fn read_head_for_summary(path: &Path) -> io::Result<Vec<serde_json::Value>> {
    use tokio::io::AsyncBufReadExt;

    let file = tokio::fs::File::open(path).await?;
    let reader = tokio::io::BufReader::new(file);
    let mut lines = reader.lines();
    let mut head = Vec::new();
    let mut saw_session_meta = false;

    while head.len() < HEAD_RECORD_LIMIT {
        let Some(line) = lines.next_line().await? else {
            break;
        };
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Ok(rollout_line) = serde_json::from_str::<RolloutLine>(trimmed) {
            match rollout_line.item {
                RolloutItem::SessionMeta(session_meta_line) => {
                    if let Ok(value) = serde_json::to_value(session_meta_line) {
                        head.push(value);
                        saw_session_meta = true;
                    }
                }
                RolloutItem::ResponseItem(item) => {
                    if let Ok(value) = serde_json::to_value(item) {
                        head.push(value);
                    }
                }
                RolloutItem::EventMsg(event) => {
                    if saw_session_meta
                        && praxis_state::thread_preview::event_msg_preview(&event).is_some()
                        && let Ok(value) = serde_json::to_value(event)
                    {
                        head.push(value);
                    }
                }
                RolloutItem::Compacted(_) | RolloutItem::TurnContext(_) => {}
            }
        }
    }

    Ok(head)
}

/// Read the SessionMetaLine from the head of a rollout file for reuse by
/// callers that need the session metadata (e.g. to derive a cwd for config).
pub async fn read_session_meta_line(path: &Path) -> io::Result<SessionMetaLine> {
    let head = read_head_for_summary(path).await?;
    let Some(first) = head.first() else {
        return Err(io::Error::other(format!(
            "rollout at {} is empty",
            path.display()
        )));
    };
    serde_json::from_value::<SessionMetaLine>(first.clone()).map_err(|_| {
        io::Error::other(format!(
            "rollout at {} does not start with session metadata",
            path.display()
        ))
    })
}
