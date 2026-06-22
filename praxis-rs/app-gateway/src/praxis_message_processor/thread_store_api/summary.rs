use chrono::DateTime;
use chrono::SecondsFormat;
use chrono::Utc;
use praxis_app_gateway_protocol::ThreadTokenUsage;
use praxis_core::SessionMeta;
use praxis_core::config::Config;
use praxis_core::read_head_for_summary;
use praxis_protocol::ThreadId;
use praxis_protocol::models::ResponseItem;
use praxis_protocol::protocol::EventMsg;
use praxis_protocol::protocol::GitInfo as CoreGitInfo;
use praxis_protocol::protocol::SessionMetaLine;
use praxis_protocol::protocol::SessionSource;
use praxis_protocol::protocol::SubAgentSource;
use praxis_rollout::state_db::StateDbHandle;
use praxis_state::ThreadMetadata;
use std::io::Error as IoError;
use std::path::Path;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq)]
pub(in crate::praxis_message_processor) struct ThreadStoreSummary {
    pub(in crate::praxis_message_processor) conversation_id: ThreadId,
    pub(in crate::praxis_message_processor) path: PathBuf,
    pub(in crate::praxis_message_processor) preview: String,
    pub(in crate::praxis_message_processor) summary: Option<String>,
    pub(in crate::praxis_message_processor) timestamp: Option<String>,
    pub(in crate::praxis_message_processor) updated_at: Option<String>,
    pub(in crate::praxis_message_processor) model_provider: String,
    pub(in crate::praxis_message_processor) model: Option<String>,
    pub(in crate::praxis_message_processor) cwd: PathBuf,
    pub(in crate::praxis_message_processor) cli_version: String,
    pub(in crate::praxis_message_processor) source: SessionSource,
    pub(in crate::praxis_message_processor) total_cost_usd: Option<f64>,
    pub(in crate::praxis_message_processor) last_cost_usd: Option<f64>,
    pub(in crate::praxis_message_processor) token_usage: Option<ThreadTokenUsage>,
    pub(in crate::praxis_message_processor) selfwork_plan_path: Option<PathBuf>,
    pub(in crate::praxis_message_processor) git_info: Option<ThreadStoreGitInfo>,
    pub(in crate::praxis_message_processor) thread_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub(in crate::praxis_message_processor) struct ThreadStoreGitInfo {
    pub(in crate::praxis_message_processor) sha: Option<String>,
    pub(in crate::praxis_message_processor) branch: Option<String>,
    pub(in crate::praxis_message_processor) origin_url: Option<String>,
}

impl ThreadStoreGitInfo {
    fn from_parts(sha: Option<String>, branch: Option<String>, origin_url: Option<String>) -> Self {
        Self {
            sha,
            branch,
            origin_url,
        }
    }

    fn from_optional_parts(
        sha: Option<String>,
        branch: Option<String>,
        origin_url: Option<String>,
    ) -> Option<Self> {
        if sha.is_none() && branch.is_none() && origin_url.is_none() {
            None
        } else {
            Some(Self::from_parts(sha, branch, origin_url))
        }
    }

    fn from_core_git_info(git_info: &CoreGitInfo) -> Self {
        Self::from_parts(
            git_info.commit_hash.as_ref().map(|sha| sha.0.clone()),
            git_info.branch.clone(),
            git_info.repository_url.clone(),
        )
    }
}

impl ThreadStoreSummary {
    pub(super) fn from_rollout_summary(summary: praxis_rollout::ThreadSummary) -> Self {
        Self {
            conversation_id: summary.conversation_id,
            path: summary.path,
            preview: summary.preview,
            summary: summary.summary,
            timestamp: summary.timestamp,
            updated_at: summary.updated_at,
            model_provider: summary.model_provider,
            model: summary.model,
            cwd: summary.cwd,
            cli_version: summary.cli_version,
            source: summary.source,
            total_cost_usd: cost_micros_to_usd(summary.total_cost_micros),
            last_cost_usd: cost_micros_to_usd(summary.last_cost_micros),
            token_usage: summary.token_usage_info.map(ThreadTokenUsage::from),
            selfwork_plan_path: summary.selfwork_plan_path,
            git_info: summary
                .git_info
                .map(|git| ThreadStoreGitInfo::from_parts(git.sha, git.branch, git.origin_url)),
            thread_name: summary.thread_name,
        }
    }
}

pub(super) async fn try_read_directory_summary(
    config: &Config,
    thread_id: ThreadId,
) -> std::io::Result<Option<ThreadStoreSummary>> {
    let directory = praxis_rollout::ThreadDirectory::open(config).await;
    directory
        .read_thread_summary(thread_id, None, config.model_provider_id.as_str())
        .await
        .map(|summary| summary.map(ThreadStoreSummary::from_rollout_summary))
}

pub(super) async fn read_state_db_summary(
    state_db_ctx: Option<&StateDbHandle>,
    thread_id: ThreadId,
) -> Option<ThreadStoreSummary> {
    let state_db_ctx = state_db_ctx?;
    state_db_ctx
        .get_thread(thread_id)
        .await
        .ok()
        .flatten()
        .map(|metadata| summary_from_metadata(&metadata))
}

#[allow(clippy::too_many_arguments)]
pub(super) fn summary_from_state_db_metadata(
    conversation_id: ThreadId,
    path: PathBuf,
    first_user_message: Option<String>,
    session_summary: Option<String>,
    timestamp: String,
    updated_at: String,
    model_provider: String,
    model: Option<String>,
    cwd: PathBuf,
    cli_version: String,
    source: String,
    total_cost_micros: Option<i64>,
    last_cost_micros: Option<i64>,
    token_usage_info: Option<praxis_protocol::protocol::TokenUsageInfo>,
    selfwork_plan_path: Option<PathBuf>,
    agent_base_name: Option<String>,
    agent_title: Option<String>,
    agent_display_name: Option<String>,
    agent_role: Option<String>,
    git_sha: Option<String>,
    git_branch: Option<String>,
    git_origin_url: Option<String>,
) -> ThreadStoreSummary {
    let preview = first_user_message.unwrap_or_default();
    let source = serde_json::from_str(&source)
        .or_else(|_| serde_json::from_value(serde_json::Value::String(source.clone())))
        .unwrap_or(SessionSource::Unknown);
    let source = with_thread_spawn_agent_metadata(
        source,
        agent_base_name,
        agent_title,
        agent_display_name,
        agent_role,
    );
    let git_info = ThreadStoreGitInfo::from_optional_parts(git_sha, git_branch, git_origin_url);
    ThreadStoreSummary {
        conversation_id,
        path,
        preview,
        summary: session_summary,
        timestamp: Some(timestamp),
        updated_at: Some(updated_at),
        model_provider,
        model,
        cwd,
        cli_version,
        source,
        total_cost_usd: cost_micros_to_usd(total_cost_micros),
        last_cost_usd: cost_micros_to_usd(last_cost_micros),
        token_usage: token_usage_info.map(ThreadTokenUsage::from),
        selfwork_plan_path,
        git_info,
        thread_name: None,
    }
}

pub(super) fn summary_from_metadata(metadata: &ThreadMetadata) -> ThreadStoreSummary {
    summary_from_state_db_metadata(
        metadata.id,
        metadata.rollout_path.clone(),
        metadata.first_user_message.clone(),
        metadata.session_summary.clone(),
        metadata
            .created_at
            .to_rfc3339_opts(SecondsFormat::Secs, true),
        metadata
            .updated_at
            .to_rfc3339_opts(SecondsFormat::Secs, true),
        metadata.model_provider.clone(),
        metadata.model.clone(),
        metadata.cwd.clone(),
        metadata.cli_version.clone(),
        metadata.source.clone(),
        metadata.total_cost_micros,
        metadata.last_cost_micros,
        metadata.token_usage_info.clone(),
        metadata.selfwork_plan_path.clone(),
        metadata.agent_base_name.clone(),
        metadata.agent_title.clone(),
        metadata.agent_display_name.clone(),
        metadata.agent_role.clone(),
        metadata.git_sha.clone(),
        metadata.git_branch.clone(),
        metadata.git_origin_url.clone(),
    )
}

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

fn non_empty_timestamp(timestamp: &str) -> Option<String> {
    non_empty_timestamp_str(timestamp).map(str::to_string)
}

fn non_empty_timestamp_str(timestamp: &str) -> Option<&str> {
    if timestamp.is_empty() {
        None
    } else {
        Some(timestamp)
    }
}

fn rollout_preview_from_summary_values(head: &[serde_json::Value]) -> Option<String> {
    head.iter()
        .find_map(thread_preview_from_summary_value)
        .map(praxis_state::thread_preview::ThreadUserPreview::into_display_text)
}

fn thread_preview_from_summary_value(
    value: &serde_json::Value,
) -> Option<praxis_state::thread_preview::ThreadUserPreview> {
    serde_json::from_value::<ResponseItem>(value.clone())
        .ok()
        .and_then(|item| praxis_state::thread_preview::response_item_preview(&item))
        .or_else(|| {
            serde_json::from_value::<EventMsg>(value.clone())
                .ok()
                .and_then(|event| praxis_state::thread_preview::event_msg_preview(&event))
        })
}

fn with_thread_spawn_agent_metadata(
    source: SessionSource,
    agent_base_name: Option<String>,
    agent_title: Option<String>,
    agent_display_name: Option<String>,
    agent_role: Option<String>,
) -> SessionSource {
    if agent_base_name.is_none()
        && agent_title.is_none()
        && agent_display_name.is_none()
        && agent_role.is_none()
    {
        return source;
    }

    match source {
        SessionSource::SubAgent(SubAgentSource::ThreadSpawn {
            parent_thread_id,
            depth,
            agent_path,
            agent_base_name: existing_agent_base_name,
            agent_title: existing_agent_title,
            agent_display_name: existing_agent_display_name,
            agent_role: existing_agent_role,
        }) => SessionSource::SubAgent(SubAgentSource::ThreadSpawn {
            parent_thread_id,
            depth,
            agent_path,
            agent_base_name: agent_base_name.or(existing_agent_base_name),
            agent_title: agent_title.or(existing_agent_title),
            agent_display_name: agent_display_name.or(existing_agent_display_name),
            agent_role: agent_role.or(existing_agent_role),
        }),
        _ => source,
    }
}

async fn read_updated_at(path: &Path, created_at: Option<&str>) -> Option<String> {
    let updated_at = tokio::fs::metadata(path)
        .await
        .ok()
        .and_then(|meta| meta.modified().ok())
        .map(|modified| {
            let updated_at: DateTime<Utc> = modified.into();
            updated_at.to_rfc3339_opts(SecondsFormat::Secs, true)
        });
    updated_at.or_else(|| created_at.map(str::to_string))
}

fn cost_micros_to_usd(value: Option<i64>) -> Option<f64> {
    value.map(|micros| micros as f64 / 1_000_000.0)
}
