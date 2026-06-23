use super::ThreadStoreGitInfo;
use super::ThreadStoreSummary;
use super::money::cost_micros_to_usd;
use super::source_metadata::with_thread_spawn_agent_metadata;
use chrono::SecondsFormat;
use praxis_app_gateway_protocol::ThreadTokenUsage;
use praxis_protocol::ThreadId;
use praxis_protocol::protocol::SessionSource;
use praxis_rollout::state_db::StateDbHandle;
use praxis_state::ThreadMetadata;
use std::path::PathBuf;

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
