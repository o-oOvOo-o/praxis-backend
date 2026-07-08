use super::thread_store_api::ThreadStore;
use super::thread_store_api::ThreadStoreSummary;
use super::thread_store_api::ThreadTurnHydration;
use chrono::DateTime;
use chrono::Utc;
use praxis_app_gateway_protocol::GitInfo as ApiGitInfo;
use praxis_app_gateway_protocol::Thread;
use praxis_app_gateway_protocol::ThreadStatus;
use praxis_core::ThreadConfigSnapshot;
use praxis_core::config::Config;
use praxis_protocol::ThreadId;
use praxis_rollout::state_db::StateDbHandle;
use praxis_state::ThreadMetadata;
use std::path::Path;
use std::path::PathBuf;

pub(super) async fn load_thread_summary_for_rollout(
    config: &Config,
    thread_id: ThreadId,
    rollout_path: &Path,
    fallback_provider: &str,
    persisted_metadata: Option<&ThreadMetadata>,
) -> std::result::Result<Thread, String> {
    let mut thread = ThreadStore::read_rollout_summary(rollout_path, fallback_provider)
        .await
        .map(summary_to_thread)
        .map_err(|err| {
            format!(
                "failed to load rollout `{}` for thread {thread_id}: {err}",
                rollout_path.display()
            )
        })?;
    if let Some(persisted_metadata) = persisted_metadata {
        merge_mutable_thread_metadata(
            &mut thread,
            summary_to_thread(ThreadStore::summary_from_metadata(persisted_metadata)),
        );
    } else if let Some(summary) = ThreadStore::new(config)
        .read_directory_summary(thread_id)
        .await
    {
        merge_mutable_thread_metadata(&mut thread, summary_to_thread(summary));
    }
    Ok(thread)
}

async fn project_thread_from_rollout_summary(
    rollout_path: &Path,
    fallback_provider: &str,
) -> std::io::Result<Thread> {
    ThreadStore::read_rollout_summary(rollout_path, fallback_provider)
        .await
        .map(summary_to_thread)
}

pub(crate) async fn project_rollback_thread_from_rollout(
    rollout_path: &Path,
    fallback_provider: &str,
    praxis_home: &Path,
    thread_id: &ThreadId,
) -> std::result::Result<Thread, String> {
    let mut thread = project_thread_from_rollout_summary(rollout_path, fallback_provider)
        .await
        .map_err(|err| format!("failed to load rollout `{}`: {err}", rollout_path.display()))?;
    thread.turns = ThreadStore::read_turns_from_rollout(rollout_path, ThreadTurnHydration::all())
        .await
        .map_err(|err| format!("failed to load rollout `{}`: {err}", rollout_path.display()))?;
    thread.name = ThreadStore::resolve_thread_name_from_home(praxis_home, thread_id.clone()).await;
    Ok(thread)
}

pub(super) async fn load_thread_summary_from_state_db_context(
    state_db_ctx: Option<&StateDbHandle>,
    thread_id: ThreadId,
) -> Option<Thread> {
    ThreadStore::read_state_db_summary(state_db_ctx, thread_id)
        .await
        .map(summary_to_thread)
}

fn merge_mutable_thread_metadata(thread: &mut Thread, persisted_thread: Thread) {
    thread.git_info = persisted_thread.git_info;
    thread.summary = persisted_thread.summary;
    thread.total_cost_usd = persisted_thread.total_cost_usd;
    thread.last_cost_usd = persisted_thread.last_cost_usd;
    thread.token_usage = persisted_thread.token_usage;
    thread.selfwork_plan_path = persisted_thread.selfwork_plan_path;
    thread.name = persisted_thread.name;
}

fn parse_datetime(timestamp: Option<&str>) -> Option<DateTime<Utc>> {
    timestamp.and_then(|ts| {
        chrono::DateTime::parse_from_rfc3339(ts)
            .ok()
            .map(|dt| dt.with_timezone(&chrono::Utc))
    })
}

pub(super) fn build_thread_from_snapshot(
    thread_id: ThreadId,
    config_snapshot: &ThreadConfigSnapshot,
    path: Option<PathBuf>,
) -> Thread {
    let now = time::OffsetDateTime::now_utc().unix_timestamp();
    Thread {
        id: thread_id.to_string(),
        preview: String::new(),
        summary: None,
        ephemeral: config_snapshot.ephemeral,
        model_provider: config_snapshot.model_provider_id.clone(),
        model: Some(config_snapshot.model.clone()),
        created_at: now,
        updated_at: now,
        status: ThreadStatus::NotLoaded,
        path,
        cwd: config_snapshot.cwd.clone(),
        cli_version: env!("CARGO_PKG_VERSION").to_string(),
        agent_base_name: config_snapshot.session_source.get_agent_base_name(),
        agent_title: config_snapshot.session_source.get_agent_title(),
        agent_display_name: config_snapshot.session_source.get_agent_display_name(),
        agent_role: config_snapshot.session_source.get_agent_role(),
        source: config_snapshot.session_source.clone().into(),
        git_info: None,
        name: None,
        total_cost_usd: None,
        last_cost_usd: None,
        token_usage: None,
        control_state: None,
        selfwork_plan_path: None,
        turns: Vec::new(),
    }
}

pub(super) fn summary_to_thread(summary: ThreadStoreSummary) -> Thread {
    let ThreadStoreSummary {
        conversation_id,
        path,
        preview,
        summary,
        timestamp,
        updated_at,
        model_provider,
        model,
        cwd,
        cli_version,
        source,
        total_cost_usd,
        last_cost_usd,
        token_usage,
        selfwork_plan_path,
        git_info,
        thread_name,
    } = summary;

    let created_at = parse_datetime(timestamp.as_deref());
    let updated_at = parse_datetime(updated_at.as_deref()).or(created_at);
    let git_info = git_info.map(|info| ApiGitInfo {
        sha: info.sha,
        branch: info.branch,
        origin_url: info.origin_url,
    });

    Thread {
        id: conversation_id.to_string(),
        preview,
        summary,
        ephemeral: false,
        model_provider,
        model,
        created_at: created_at.map(|dt| dt.timestamp()).unwrap_or(0),
        updated_at: updated_at.map(|dt| dt.timestamp()).unwrap_or(0),
        status: ThreadStatus::NotLoaded,
        path: Some(path),
        cwd,
        cli_version,
        agent_base_name: source.get_agent_base_name(),
        agent_title: source.get_agent_title(),
        agent_display_name: source.get_agent_display_name(),
        agent_role: source.get_agent_role(),
        source: source.into(),
        git_info,
        name: thread_name,
        total_cost_usd,
        last_cost_usd,
        token_usage,
        control_state: None,
        selfwork_plan_path,
        turns: Vec::new(),
    }
}
