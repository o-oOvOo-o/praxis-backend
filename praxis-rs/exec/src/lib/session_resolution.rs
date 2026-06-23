use super::*;

pub(super) fn lagged_event_warning_message(skipped: usize) -> String {
    format!("app-gateway event stream lagged; dropped {skipped} events")
}

pub(super) fn should_process_notification(
    notification: &ServerNotification,
    thread_id: &str,
    turn_id: &str,
) -> bool {
    match notification {
        ServerNotification::ConfigWarning(_) | ServerNotification::DeprecationNotice(_) => true,
        ServerNotification::Error(notification) => {
            notification.thread_id == thread_id && notification.turn_id == turn_id
        }
        ServerNotification::HookCompleted(notification) => {
            notification.thread_id == thread_id
                && notification
                    .turn_id
                    .as_deref()
                    .is_none_or(|candidate| candidate == turn_id)
        }
        ServerNotification::HookStarted(notification) => {
            notification.thread_id == thread_id
                && notification
                    .turn_id
                    .as_deref()
                    .is_none_or(|candidate| candidate == turn_id)
        }
        ServerNotification::ItemCompleted(notification) => {
            notification.thread_id == thread_id && notification.turn_id == turn_id
        }
        ServerNotification::ItemStarted(notification) => {
            notification.thread_id == thread_id && notification.turn_id == turn_id
        }
        ServerNotification::ModelRerouted(notification) => {
            notification.thread_id == thread_id && notification.turn_id == turn_id
        }
        ServerNotification::ThreadTokenUsageUpdated(notification) => {
            notification.thread_id == thread_id && notification.turn_id == turn_id
        }
        ServerNotification::TurnCompleted(notification) => {
            notification.thread_id == thread_id && notification.turn.id == turn_id
        }
        ServerNotification::TurnDiffUpdated(notification) => {
            notification.thread_id == thread_id && notification.turn_id == turn_id
        }
        ServerNotification::TurnPlanUpdated(notification) => {
            notification.thread_id == thread_id && notification.turn_id == turn_id
        }
        ServerNotification::TurnStarted(notification) => {
            notification.thread_id == thread_id && notification.turn.id == turn_id
        }
        _ => false,
    }
}

pub(super) async fn maybe_backfill_turn_completed_items(
    client: &AppGatewayClient,
    request_ids: &mut RequestIdSequencer,
    notification: &mut ServerNotification,
) {
    // In-process delivery may drop non-terminal item notifications under backpressure while still
    // guaranteeing `turn/completed`. Because app-gateway currently emits that completion with an
    // empty `turn.items`, exec does one last `thread/read` here so human/json output can recover
    // the final message and reconcile any still-running items before shutdown.
    let ServerNotification::TurnCompleted(payload) = notification else {
        return;
    };
    if !payload.turn.items.is_empty() {
        return;
    }

    let response = send_request_with_response::<ThreadReadResponse>(
        client,
        ClientRequest::ThreadRead {
            request_id: request_ids.next(),
            params: ThreadReadParams {
                thread_id: payload.thread_id.clone(),
                include_turns: true,
            },
        },
        "thread/read",
    )
    .await;

    match response {
        Ok(response) => {
            if let Some(items) = turn_items_for_thread(&response.thread, &payload.turn.id) {
                payload.turn.items = items;
            }
        }
        Err(err) => {
            warn!("thread/read failed while backfilling turn items for turn completion: {err}");
        }
    }
}

pub(super) fn turn_items_for_thread(
    thread: &AppGatewayThread,
    turn_id: &str,
) -> Option<Vec<AppGatewayThreadItem>> {
    thread
        .turns
        .iter()
        .find(|turn| turn.id == turn_id)
        .map(|turn| turn.items.clone())
}

pub(super) fn all_thread_source_kinds() -> Vec<ThreadSourceKind> {
    vec![
        ThreadSourceKind::Cli,
        ThreadSourceKind::VsCode,
        ThreadSourceKind::Exec,
        ThreadSourceKind::AppGateway,
        ThreadSourceKind::SubAgent,
        ThreadSourceKind::SubAgentReview,
        ThreadSourceKind::SubAgentCompact,
        ThreadSourceKind::SubAgentThreadSpawn,
        ThreadSourceKind::SubAgentOther,
        ThreadSourceKind::Unknown,
    ]
}

pub(super) async fn latest_thread_cwd(thread: &AppGatewayThread) -> PathBuf {
    if let Some(path) = thread.path.as_deref()
        && let Some(cwd) = parse_latest_turn_context_cwd(path).await
    {
        return cwd;
    }
    thread.cwd.clone()
}

pub(super) async fn parse_latest_turn_context_cwd(path: &Path) -> Option<PathBuf> {
    let text = tokio::fs::read_to_string(path).await.ok()?;
    for line in text.lines().rev() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Ok(rollout_line) = serde_json::from_str::<RolloutLine>(trimmed) else {
            continue;
        };
        if let RolloutItem::TurnContext(item) = rollout_line.item {
            return Some(item.cwd);
        }
    }
    None
}

pub(super) fn cwds_match(current_cwd: &Path, session_cwd: &Path) -> bool {
    match (
        path_utils::normalize_for_path_comparison(current_cwd),
        path_utils::normalize_for_path_comparison(session_cwd),
    ) {
        (Ok(current), Ok(session)) => current == session,
        _ => current_cwd == session_cwd,
    }
}

pub(super) async fn resolve_resume_thread_id(
    client: &AppGatewayClient,
    config: &Config,
    args: &crate::cli::ResumeArgs,
) -> anyhow::Result<Option<String>> {
    let model_providers = resume_lookup_model_providers(config, args);

    if args.last {
        let mut cursor = None;
        loop {
            let response: ThreadListResponse = send_request_with_response(
                client,
                ClientRequest::ThreadList {
                    request_id: RequestId::Integer(0),
                    params: ThreadListParams {
                        cursor,
                        limit: Some(100),
                        sort_key: Some(ThreadSortKey::UpdatedAt),
                        model_providers: model_providers.clone(),
                        source_kinds: Some(all_thread_source_kinds()),
                        archived: Some(false),
                        cwd: None,
                        cwd_scope: None,
                        search_term: None,
                    },
                },
                "thread/list",
            )
            .await
            .map_err(anyhow::Error::msg)?;
            for thread in response.data {
                if args.all || cwds_match(config.cwd.as_path(), &latest_thread_cwd(&thread).await) {
                    return Ok(Some(thread.id));
                }
            }
            let Some(next_cursor) = response.next_cursor else {
                return Ok(None);
            };
            cursor = Some(next_cursor);
        }
    }

    let Some(session_id) = args.session_id.as_deref() else {
        return Ok(None);
    };
    if Uuid::parse_str(session_id).is_ok() {
        return Ok(Some(session_id.to_string()));
    }

    let mut cursor = None;
    loop {
        let response: ThreadListResponse = send_request_with_response(
            client,
            ClientRequest::ThreadList {
                request_id: RequestId::Integer(0),
                params: ThreadListParams {
                    cursor,
                    limit: Some(100),
                    sort_key: Some(ThreadSortKey::UpdatedAt),
                    model_providers: model_providers.clone(),
                    source_kinds: Some(all_thread_source_kinds()),
                    archived: Some(false),
                    cwd: None,
                    cwd_scope: None,
                    // Thread names are attached separately from rollout titles, so name
                    // resolution must scan the filtered list client-side instead of relying
                    // on the backend `search_term` filter.
                    search_term: None,
                },
            },
            "thread/list",
        )
        .await
        .map_err(anyhow::Error::msg)?;
        for thread in response.data {
            if thread.name.as_deref() != Some(session_id) {
                continue;
            }
            if args.all || cwds_match(config.cwd.as_path(), &latest_thread_cwd(&thread).await) {
                return Ok(Some(thread.id));
            }
        }
        let Some(next_cursor) = response.next_cursor else {
            return Ok(None);
        };
        cursor = Some(next_cursor);
    }
}

pub(super) fn resume_lookup_model_providers(
    config: &Config,
    args: &crate::cli::ResumeArgs,
) -> Option<Vec<String>> {
    if args.last {
        Some(vec![config.model_provider_id.clone()])
    } else {
        None
    }
}
