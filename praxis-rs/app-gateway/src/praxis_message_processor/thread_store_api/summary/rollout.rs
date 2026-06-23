use super::ThreadStoreGitInfo;
use super::ThreadStoreSummary;
use super::preview::rollout_preview_from_summary_values;
use super::source_metadata::with_thread_spawn_agent_metadata;
use super::timestamp::non_empty_timestamp;
use super::timestamp::non_empty_timestamp_str;
use super::timestamp::read_updated_at;
use praxis_core::SessionMeta;
use praxis_core::read_head_for_summary;
#[cfg(test)]
use praxis_protocol::protocol::GitInfo as CoreGitInfo;
use praxis_protocol::protocol::SessionMetaLine;
use std::io::Error as IoError;
use std::path::Path;
use std::path::PathBuf;

pub(super) async fn read_summary_from_rollout(
    path: &Path,
    fallback_provider: &str,
) -> std::io::Result<ThreadStoreSummary> {
    let head = read_head_for_summary(path).await?;

    let Some(first) = head.first() else {
        return Err(IoError::other(format!(
            "rollout at {} is empty",
            path.display()
        )));
    };

    let session_meta_line =
        serde_json::from_value::<SessionMetaLine>(first.clone()).map_err(|_| {
            IoError::other(format!(
                "rollout at {} does not start with session metadata",
                path.display()
            ))
        })?;
    let SessionMetaLine {
        meta: session_meta,
        git,
    } = session_meta_line;
    let mut session_meta = session_meta;
    session_meta.source = with_thread_spawn_agent_metadata(
        session_meta.source.clone(),
        session_meta.agent_base_name.clone(),
        session_meta.agent_title.clone(),
        session_meta.agent_display_name.clone(),
        session_meta.agent_role.clone(),
    );

    let created_at = non_empty_timestamp_str(&session_meta.timestamp);
    let updated_at = read_updated_at(path, created_at).await;
    let preview = rollout_preview_from_summary_values(&head).unwrap_or_default();

    Ok(rollout_summary_from_session_meta(
        path.to_path_buf(),
        preview,
        &session_meta,
        git.as_ref().map(ThreadStoreGitInfo::from_core_git_info),
        fallback_provider,
        updated_at,
    ))
}

#[cfg(test)]
pub(super) fn extract_rollout_summary(
    path: PathBuf,
    head: &[serde_json::Value],
    session_meta: &SessionMeta,
    git: Option<&CoreGitInfo>,
    fallback_provider: &str,
    updated_at: Option<String>,
) -> Option<ThreadStoreSummary> {
    let preview = rollout_preview_from_summary_values(head)?;

    Some(rollout_summary_from_session_meta(
        path,
        preview,
        session_meta,
        git.map(ThreadStoreGitInfo::from_core_git_info),
        fallback_provider,
        updated_at,
    ))
}

fn rollout_summary_from_session_meta(
    path: PathBuf,
    preview: String,
    session_meta: &SessionMeta,
    git_info: Option<ThreadStoreGitInfo>,
    fallback_provider: &str,
    updated_at: Option<String>,
) -> ThreadStoreSummary {
    let timestamp = non_empty_timestamp(&session_meta.timestamp);
    let updated_at = updated_at.or_else(|| timestamp.clone());

    ThreadStoreSummary {
        conversation_id: session_meta.id,
        timestamp,
        updated_at,
        path,
        preview,
        summary: None,
        model_provider: rollout_model_provider(session_meta, fallback_provider),
        model: None,
        cwd: session_meta.cwd.clone(),
        cli_version: session_meta.cli_version.clone(),
        source: session_meta.source.clone(),
        total_cost_usd: None,
        last_cost_usd: None,
        token_usage: None,
        selfwork_plan_path: None,
        git_info,
        thread_name: None,
    }
}

fn rollout_model_provider(session_meta: &SessionMeta, fallback_provider: &str) -> String {
    session_meta
        .model_provider
        .clone()
        .unwrap_or_else(|| fallback_provider.to_string())
}
