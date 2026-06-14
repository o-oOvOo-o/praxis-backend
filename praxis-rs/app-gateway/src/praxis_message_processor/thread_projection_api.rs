use super::*;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct RolloutSummary {
    pub(crate) conversation_id: ThreadId,
    pub(crate) path: PathBuf,
    pub(crate) preview: String,
    pub(crate) summary: Option<String>,
    pub(crate) timestamp: Option<String>,
    pub(crate) updated_at: Option<String>,
    pub(crate) model_provider: String,
    pub(crate) model: Option<String>,
    pub(crate) cwd: PathBuf,
    pub(crate) cli_version: String,
    pub(crate) source: SessionSource,
    pub(crate) total_cost_usd: Option<f64>,
    pub(crate) last_cost_usd: Option<f64>,
    pub(crate) token_usage: Option<ThreadTokenUsage>,
    pub(crate) selfwork_plan_path: Option<PathBuf>,
    pub(crate) git_info: Option<RolloutGitInfo>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct RolloutGitInfo {
    pub(crate) sha: Option<String>,
    pub(crate) branch: Option<String>,
    pub(crate) origin_url: Option<String>,
}

async fn read_summary_from_thread_directory_by_thread_id(
    config: &Config,
    thread_id: ThreadId,
) -> Option<RolloutSummary> {
    let directory = praxis_rollout::ThreadDirectory::open(config).await;
    directory
        .read_thread_summary(thread_id, None, config.model_provider_id.as_str())
        .await
        .ok()
        .flatten()
        .map(thread_summary_to_rollout_summary)
}

pub(crate) async fn read_summary_from_state_db_context_by_thread_id(
    state_db_ctx: Option<&StateDbHandle>,
    thread_id: ThreadId,
) -> Option<RolloutSummary> {
    let state_db_ctx = state_db_ctx?;

    let metadata = match state_db_ctx.get_thread(thread_id).await {
        Ok(Some(metadata)) => metadata,
        Ok(None) | Err(_) => return None,
    };
    Some(summary_from_thread_metadata(&metadata))
}

pub(crate) async fn hydrate_rollout_summary_with_state_db(
    config: &Config,
    mut summary: RolloutSummary,
) -> RolloutSummary {
    if let Some(persisted) =
        read_summary_from_thread_directory_by_thread_id(config, summary.conversation_id).await
    {
        merge_mutable_summary_metadata(&mut summary, persisted);
    }
    summary
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn summary_from_state_db_metadata(
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
) -> RolloutSummary {
    let preview = first_user_message.unwrap_or_default();
    let source = serde_json::from_str(&source)
        .or_else(|_| serde_json::from_value(serde_json::Value::String(source.clone())))
        .unwrap_or(praxis_protocol::protocol::SessionSource::Unknown);
    let source = with_thread_spawn_agent_metadata(
        source,
        agent_base_name,
        agent_title,
        agent_display_name,
        agent_role,
    );
    let git_info = if git_sha.is_none() && git_branch.is_none() && git_origin_url.is_none() {
        None
    } else {
        Some(RolloutGitInfo {
            sha: git_sha,
            branch: git_branch,
            origin_url: git_origin_url,
        })
    };
    RolloutSummary {
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
    }
}

fn summary_from_thread_metadata(metadata: &ThreadMetadata) -> RolloutSummary {
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

pub(crate) fn thread_summary_to_rollout_summary(
    summary: praxis_rollout::ThreadSummary,
) -> RolloutSummary {
    RolloutSummary {
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
        git_info: summary.git_info.map(|git| RolloutGitInfo {
            sha: git.sha,
            branch: git.branch,
            origin_url: git.origin_url,
        }),
    }
}

fn merge_mutable_summary_metadata(summary: &mut RolloutSummary, persisted_summary: RolloutSummary) {
    summary.git_info = persisted_summary.git_info;
    summary.summary = persisted_summary.summary;
    summary.total_cost_usd = persisted_summary.total_cost_usd;
    summary.last_cost_usd = persisted_summary.last_cost_usd;
    summary.token_usage = persisted_summary.token_usage;
    summary.selfwork_plan_path = persisted_summary.selfwork_plan_path;
}

pub(crate) async fn read_summary_from_rollout(
    path: &Path,
    fallback_provider: &str,
) -> std::io::Result<RolloutSummary> {
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

    let created_at = if session_meta.timestamp.is_empty() {
        None
    } else {
        Some(session_meta.timestamp.as_str())
    };
    let updated_at = read_updated_at(path, created_at).await;
    if let Some(summary) = extract_rollout_summary(
        path.to_path_buf(),
        &head,
        &session_meta,
        git.as_ref(),
        fallback_provider,
        updated_at.clone(),
    ) {
        return Ok(summary);
    }

    let timestamp = if session_meta.timestamp.is_empty() {
        None
    } else {
        Some(session_meta.timestamp.clone())
    };
    let model_provider = session_meta
        .model_provider
        .clone()
        .unwrap_or_else(|| fallback_provider.to_string());
    let git_info = git.as_ref().map(map_git_info);
    let updated_at = updated_at.or_else(|| timestamp.clone());

    Ok(RolloutSummary {
        conversation_id: session_meta.id,
        timestamp,
        updated_at,
        path: path.to_path_buf(),
        preview: String::new(),
        summary: None,
        model_provider,
        model: None,
        cwd: session_meta.cwd,
        cli_version: session_meta.cli_version,
        source: session_meta.source,
        total_cost_usd: None,
        last_cost_usd: None,
        token_usage: None,
        selfwork_plan_path: None,
        git_info,
    })
}

pub(crate) fn extract_rollout_summary(
    path: PathBuf,
    head: &[serde_json::Value],
    session_meta: &SessionMeta,
    git: Option<&CoreGitInfo>,
    fallback_provider: &str,
    updated_at: Option<String>,
) -> Option<RolloutSummary> {
    let preview = head.iter().find_map(thread_preview_from_summary_value)?;

    let timestamp = if session_meta.timestamp.is_empty() {
        None
    } else {
        Some(session_meta.timestamp.clone())
    };
    let conversation_id = session_meta.id;
    let model_provider = session_meta
        .model_provider
        .clone()
        .unwrap_or_else(|| fallback_provider.to_string());
    let git_info = git.map(map_git_info);
    let updated_at = updated_at.or_else(|| timestamp.clone());

    Some(RolloutSummary {
        conversation_id,
        timestamp,
        updated_at,
        path,
        preview: preview.into_display_text(),
        summary: None,
        model_provider,
        model: None,
        cwd: session_meta.cwd.clone(),
        cli_version: session_meta.cli_version.clone(),
        source: session_meta.source.clone(),
        total_cost_usd: None,
        last_cost_usd: None,
        token_usage: None,
        selfwork_plan_path: None,
        git_info,
    })
}

fn map_git_info(git_info: &CoreGitInfo) -> RolloutGitInfo {
    RolloutGitInfo {
        sha: git_info.commit_hash.as_ref().map(|sha| sha.0.clone()),
        branch: git_info.branch.clone(),
        origin_url: git_info.repository_url.clone(),
    }
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

pub(crate) async fn load_thread_summary_for_rollout(
    config: &Config,
    thread_id: ThreadId,
    rollout_path: &Path,
    fallback_provider: &str,
    persisted_metadata: Option<&ThreadMetadata>,
) -> std::result::Result<Thread, String> {
    let mut thread = read_summary_from_rollout(rollout_path, fallback_provider)
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
            summary_to_thread(summary_from_thread_metadata(persisted_metadata)),
        );
    } else if let Some(summary) =
        read_summary_from_thread_directory_by_thread_id(config, thread_id).await
    {
        merge_mutable_thread_metadata(&mut thread, summary_to_thread(summary));
    }
    Ok(thread)
}

fn merge_mutable_thread_metadata(thread: &mut Thread, persisted_thread: Thread) {
    thread.git_info = persisted_thread.git_info;
    thread.summary = persisted_thread.summary;
    thread.total_cost_usd = persisted_thread.total_cost_usd;
    thread.last_cost_usd = persisted_thread.last_cost_usd;
    thread.token_usage = persisted_thread.token_usage;
    thread.selfwork_plan_path = persisted_thread.selfwork_plan_path;
}

pub(crate) fn preview_from_rollout_items(items: &[RolloutItem]) -> String {
    items
        .iter()
        .find_map(praxis_state::thread_preview::rollout_item_preview)
        .map(praxis_state::thread_preview::ThreadUserPreview::into_display_text)
        .unwrap_or_default()
}

fn with_thread_spawn_agent_metadata(
    source: praxis_protocol::protocol::SessionSource,
    agent_base_name: Option<String>,
    agent_title: Option<String>,
    agent_display_name: Option<String>,
    agent_role: Option<String>,
) -> praxis_protocol::protocol::SessionSource {
    if agent_base_name.is_none()
        && agent_title.is_none()
        && agent_display_name.is_none()
        && agent_role.is_none()
    {
        return source;
    }

    match source {
        praxis_protocol::protocol::SessionSource::SubAgent(
            praxis_protocol::protocol::SubAgentSource::ThreadSpawn {
                parent_thread_id,
                depth,
                agent_path,
                agent_base_name: existing_agent_base_name,
                agent_title: existing_agent_title,
                agent_display_name: existing_agent_display_name,
                agent_role: existing_agent_role,
            },
        ) => praxis_protocol::protocol::SessionSource::SubAgent(
            praxis_protocol::protocol::SubAgentSource::ThreadSpawn {
                parent_thread_id,
                depth,
                agent_path,
                agent_base_name: agent_base_name.or(existing_agent_base_name),
                agent_title: agent_title.or(existing_agent_title),
                agent_display_name: agent_display_name.or(existing_agent_display_name),
                agent_role: agent_role.or(existing_agent_role),
            },
        ),
        _ => source,
    }
}

fn parse_datetime(timestamp: Option<&str>) -> Option<DateTime<Utc>> {
    timestamp.and_then(|ts| {
        chrono::DateTime::parse_from_rfc3339(ts)
            .ok()
            .map(|dt| dt.with_timezone(&chrono::Utc))
    })
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

pub(crate) fn build_thread_from_snapshot(
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

pub(crate) fn summary_to_thread(summary: RolloutSummary) -> Thread {
    let RolloutSummary {
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
        name: None,
        total_cost_usd,
        last_cost_usd,
        token_usage,
        control_state: None,
        selfwork_plan_path,
        turns: Vec::new(),
    }
}

fn cost_micros_to_usd(value: Option<i64>) -> Option<f64> {
    value.map(|micros| micros as f64 / 1_000_000.0)
}
