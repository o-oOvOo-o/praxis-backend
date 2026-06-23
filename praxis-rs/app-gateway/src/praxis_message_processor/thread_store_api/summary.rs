use praxis_app_gateway_protocol::ThreadTokenUsage;
#[cfg(test)]
use praxis_core::SessionMeta;
use praxis_core::config::Config;
use praxis_protocol::ThreadId;
use praxis_protocol::protocol::GitInfo as CoreGitInfo;
use praxis_protocol::protocol::SessionSource;
use praxis_rollout::state_db::StateDbHandle;
use praxis_state::ThreadMetadata;
use std::path::Path;
use std::path::PathBuf;

mod money;
mod preview;
mod rollout;
mod source_metadata;
mod state_db;
mod timestamp;

use money::cost_micros_to_usd;

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
    pub(super) fn from_parts(
        sha: Option<String>,
        branch: Option<String>,
        origin_url: Option<String>,
    ) -> Self {
        Self {
            sha,
            branch,
            origin_url,
        }
    }

    pub(super) fn from_optional_parts(
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

    pub(super) fn from_core_git_info(git_info: &CoreGitInfo) -> Self {
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
    state_db::read_state_db_summary(state_db_ctx, thread_id).await
}

#[cfg(test)]
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
    state_db::summary_from_state_db_metadata(
        conversation_id,
        path,
        first_user_message,
        session_summary,
        timestamp,
        updated_at,
        model_provider,
        model,
        cwd,
        cli_version,
        source,
        total_cost_micros,
        last_cost_micros,
        token_usage_info,
        selfwork_plan_path,
        agent_base_name,
        agent_title,
        agent_display_name,
        agent_role,
        git_sha,
        git_branch,
        git_origin_url,
    )
}

pub(super) fn summary_from_metadata(metadata: &ThreadMetadata) -> ThreadStoreSummary {
    state_db::summary_from_metadata(metadata)
}

pub(super) async fn read_summary_from_rollout(
    path: &Path,
    fallback_provider: &str,
) -> std::io::Result<ThreadStoreSummary> {
    rollout::read_summary_from_rollout(path, fallback_provider).await
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
    rollout::extract_rollout_summary(path, head, session_meta, git, fallback_provider, updated_at)
}
