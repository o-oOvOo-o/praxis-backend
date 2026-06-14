use crate::praxis::Session;
use crate::praxis::SessionSettingsUpdate;
use crate::praxis::SteerInputError;

use crate::SkillError;
use crate::config::Config;
use crate::config_loader::CloudConfigBundleLoader;
use crate::config_loader::LoaderOverrides;
use crate::config_loader::load_config_layers_state;
use crate::praxis::spawn_review_thread;
use praxis_features::Feature;
use praxis_utils_absolute_path::AbsolutePathBuf;

use crate::review_prompts::resolve_review_request;
use crate::rollout::RolloutRecorder;
use crate::tasks::CompactTask;
use crate::tasks::UndoTask;
use crate::tasks::UserShellCommandMode;
use crate::tasks::UserShellCommandTask;
use crate::tasks::execute_user_shell_command;
use praxis_mcp::mcp::auth::compute_auth_statuses;
use praxis_mcp::mcp::collect_mcp_snapshot_from_manager;
use praxis_protocol::protocol::CodexErrorInfo;
use praxis_protocol::protocol::Event;
use praxis_protocol::protocol::EventMsg;
use praxis_protocol::protocol::InterAgentCommunication;
use praxis_protocol::protocol::ListSkillsResponseEvent;
use praxis_protocol::protocol::McpServerRefreshConfig;
use praxis_protocol::protocol::Op;
use praxis_protocol::protocol::ReviewDecision;
use praxis_protocol::protocol::ReviewRequest;
use praxis_protocol::protocol::RolloutItem;
use praxis_protocol::protocol::SkillsListEntry;
use praxis_protocol::protocol::ThreadNameUpdatedEvent;
use praxis_protocol::protocol::ThreadRolledBackEvent;
use praxis_protocol::protocol::TurnAbortReason;
use praxis_protocol::request_permissions::RequestPermissionsResponse;
use praxis_protocol::request_user_input::RequestUserInputResponse;

use crate::context_manager::is_user_turn_boundary;
use praxis_protocol::config_types::CollaborationMode;
use praxis_protocol::config_types::ModeKind;
use praxis_protocol::config_types::Settings;
use praxis_protocol::dynamic_tools::DynamicToolResponse;
use praxis_protocol::mcp::RequestId as ProtocolRequestId;
use praxis_protocol::user_input::UserInput;
use praxis_rmcp_client::ElicitationAction;
use praxis_rmcp_client::ElicitationResponse;
use serde_json::Value;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::info;
use tracing::warn;

pub async fn interrupt(sess: &Arc<Session>) {
    sess.interrupt_task().await;
}

pub async fn clean_background_terminals(sess: &Arc<Session>) {
    sess.close_unified_exec_processes().await;
}

pub async fn override_turn_context(sess: &Session, sub_id: String, updates: SessionSettingsUpdate) {
    if let Err(err) = sess.update_settings(updates).await {
        sess.raw_event_emitter(sub_id)
            .error(err.to_string(), Some(CodexErrorInfo::BadRequest))
            .await;
    }
}

pub async fn user_input_or_turn(sess: &Arc<Session>, sub_id: String, op: Op) {
    let (items, updates) = match op {
        Op::UserTurn {
            cwd,
            approval_policy,
            approvals_reviewer,
            sandbox_policy,
            model,
            model_provider,
            effort,
            summary,
            service_tier,
            final_output_json_schema,
            items,
            collaboration_mode,
            personality,
        } => {
            let collaboration_mode = collaboration_mode.or_else(|| {
                Some(CollaborationMode {
                    mode: ModeKind::Default,
                    settings: Settings {
                        model: model.clone(),
                        reasoning_effort: effort,
                        developer_instructions: None,
                    },
                })
            });
            (
                items,
                SessionSettingsUpdate {
                    cwd: Some(cwd),
                    approval_policy: Some(approval_policy),
                    approvals_reviewer,
                    sandbox_policy: Some(sandbox_policy),
                    windows_sandbox_level: None,
                    model_provider,
                    collaboration_mode,
                    reasoning_summary: summary,
                    service_tier,
                    final_output_json_schema: Some(final_output_json_schema),
                    personality,
                    app_gateway_client_name: None,
                },
            )
        }
        Op::UserInput {
            items,
            final_output_json_schema,
        } => (
            items,
            SessionSettingsUpdate {
                final_output_json_schema: Some(final_output_json_schema),
                ..Default::default()
            },
        ),
        _ => unreachable!(),
    };

    let Ok(current_context) = sess.new_turn_with_sub_id(sub_id.clone(), updates).await else {
        // new_turn_with_sub_id already emits the error event.
        return;
    };
    sess.maybe_emit_unknown_model_warning_for_turn(current_context.as_ref())
        .await;
    match sess
        .steer_input(items.clone(), /*expected_turn_id*/ None)
        .await
    {
        Ok(_) => {
            crate::auto_title::maybe_apply_provisional_title(sess, &items).await;
            current_context.session_telemetry.user_prompt(&items);
        }
        Err(SteerInputError::NoActiveTurn(items)) => {
            crate::auto_title::maybe_apply_provisional_title(sess, &items).await;
            current_context.session_telemetry.user_prompt(&items);
            sess.refresh_mcp_servers_if_requested(&current_context)
                .await;
            sess.spawn_task(
                Arc::clone(&current_context),
                items,
                crate::tasks::RegularAgentTask::new(),
            )
            .await;
        }
        Err(err) => {
            sess.raw_event_emitter(sub_id)
                .error_event(err.to_error_event())
                .await;
        }
    }
}

/// Records an inter-agent assistant envelope, then lets the shared pending-work scheduler
/// decide whether an idle session should start a regular turn.
pub async fn inter_agent_communication(
    sess: &Arc<Session>,
    sub_id: String,
    communication: InterAgentCommunication,
) {
    let trigger_turn = communication.trigger_turn;
    sess.enqueue_mailbox_communication(communication);
    if trigger_turn {
        sess.maybe_start_turn_for_pending_work_with_sub_id(sub_id)
            .await;
    }
}

pub async fn run_user_shell_command(sess: &Arc<Session>, sub_id: String, command: String) {
    if let Some((turn_context, cancellation_token)) =
        sess.active_turn_context_and_cancellation_token().await
    {
        let session = Arc::clone(sess);
        tokio::spawn(async move {
            execute_user_shell_command(
                session,
                turn_context,
                command,
                cancellation_token,
                UserShellCommandMode::ActiveTurnAuxiliary,
            )
            .await;
        });
        return;
    }

    let turn_context = sess.new_default_turn_with_sub_id(sub_id).await;
    sess.spawn_task(
        Arc::clone(&turn_context),
        Vec::new(),
        UserShellCommandTask::new(command),
    )
    .await;
}

pub async fn resolve_elicitation(
    sess: &Arc<Session>,
    server_name: String,
    request_id: ProtocolRequestId,
    decision: praxis_protocol::approvals::ElicitationAction,
    content: Option<Value>,
    meta: Option<Value>,
) {
    let action = match decision {
        praxis_protocol::approvals::ElicitationAction::Accept => ElicitationAction::Accept,
        praxis_protocol::approvals::ElicitationAction::Decline => ElicitationAction::Decline,
        praxis_protocol::approvals::ElicitationAction::Cancel => ElicitationAction::Cancel,
    };
    let content = match action {
        // Preserve the legacy fallback for clients that only send an action.
        ElicitationAction::Accept => Some(content.unwrap_or_else(|| serde_json::json!({}))),
        ElicitationAction::Decline | ElicitationAction::Cancel => None,
    };
    let response = ElicitationResponse {
        action,
        content,
        meta,
    };
    let request_id = match request_id {
        ProtocolRequestId::String(value) => {
            rmcp::model::NumberOrString::String(std::sync::Arc::from(value))
        }
        ProtocolRequestId::Integer(value) => rmcp::model::NumberOrString::Number(value),
    };
    if let Err(err) = sess
        .resolve_elicitation(server_name, request_id, response)
        .await
    {
        warn!(
            error = %err,
            "failed to resolve elicitation request in session"
        );
    }
}

/// Propagate a user's exec approval decision to the session.
/// Also optionally applies an execpolicy amendment.
pub async fn exec_approval(
    sess: &Arc<Session>,
    approval_id: String,
    turn_id: Option<String>,
    decision: ReviewDecision,
) {
    let event_turn_id = turn_id.unwrap_or_else(|| approval_id.clone());
    if let ReviewDecision::ApprovedExecpolicyAmendment {
        proposed_execpolicy_amendment,
    } = &decision
    {
        match sess
            .persist_execpolicy_amendment(proposed_execpolicy_amendment)
            .await
        {
            Ok(()) => {
                sess.record_execpolicy_amendment_message(
                    &event_turn_id,
                    proposed_execpolicy_amendment,
                )
                .await;
            }
            Err(err) => {
                let message = format!("Failed to apply execpolicy amendment: {err}");
                tracing::warn!("{message}");
                sess.raw_event_emitter(event_turn_id.clone())
                    .warning(message)
                    .await;
            }
        }
    }
    match decision {
        ReviewDecision::Abort => {
            sess.interrupt_task().await;
        }
        other => sess.notify_approval(&approval_id, other).await,
    }
}

pub async fn patch_approval(sess: &Arc<Session>, id: String, decision: ReviewDecision) {
    match decision {
        ReviewDecision::Abort => {
            sess.interrupt_task().await;
        }
        other => sess.notify_approval(&id, other).await,
    }
}

pub async fn request_user_input_response(
    sess: &Arc<Session>,
    id: String,
    response: RequestUserInputResponse,
) {
    sess.notify_user_input_response(&id, response).await;
}

pub async fn request_permissions_response(
    sess: &Arc<Session>,
    id: String,
    response: RequestPermissionsResponse,
) {
    sess.notify_request_permissions_response(&id, response)
        .await;
}

pub async fn dynamic_tool_response(sess: &Arc<Session>, id: String, response: DynamicToolResponse) {
    sess.notify_dynamic_tool_response(&id, response).await;
}

pub async fn add_to_history(sess: &Arc<Session>, config: &Arc<Config>, text: String) {
    let id = sess.conversation_id;
    let config = Arc::clone(config);
    tokio::spawn(async move {
        if let Err(e) = crate::message_history::append_entry(&text, &id, &config).await {
            warn!("failed to append to message history: {e}");
        }
    });
}

pub async fn get_history_entry_request(
    sess: &Arc<Session>,
    config: &Arc<Config>,
    sub_id: String,
    offset: usize,
    log_id: u64,
) {
    let config = Arc::clone(config);
    let sess_clone = Arc::clone(sess);

    tokio::spawn(async move {
        // Run lookup in blocking thread because it does file IO + locking.
        let entry_opt = tokio::task::spawn_blocking(move || {
            crate::message_history::lookup(log_id, offset, &config)
        })
        .await
        .unwrap_or(None);

        let event = Event {
            id: sub_id,
            msg: EventMsg::GetHistoryEntryResponse(
                praxis_protocol::protocol::GetHistoryEntryResponseEvent {
                    offset,
                    log_id,
                    entry: entry_opt.map(|e| praxis_protocol::message_history::HistoryEntry {
                        conversation_id: e.session_id,
                        ts: e.ts,
                        text: e.text,
                    }),
                },
            ),
        };

        sess_clone.send_event_raw(event).await;
    });
}

pub async fn refresh_mcp_servers(sess: &Arc<Session>, refresh_config: McpServerRefreshConfig) {
    let mut guard = sess.pending_mcp_server_refresh_config.lock().await;
    *guard = Some(refresh_config);
}

pub async fn reload_user_config(sess: &Arc<Session>) {
    sess.reload_user_config_layer().await;
}

pub async fn list_mcp_tools(sess: &Session, config: &Arc<Config>, sub_id: String) {
    let mcp_connection_manager = sess.services.mcp_connection_manager.read().await;
    let auth = sess.services.auth_manager.auth().await;
    let mcp_servers = sess
        .services
        .mcp_manager
        .effective_servers(config, auth.as_ref());
    let snapshot = collect_mcp_snapshot_from_manager(
        &mcp_connection_manager,
        compute_auth_statuses(mcp_servers.iter(), config.mcp_oauth_credentials_store_mode).await,
    )
    .await;
    let event = Event {
        id: sub_id,
        msg: EventMsg::McpListToolsResponse(snapshot),
    };
    sess.send_event_raw(event).await;
}

pub async fn list_skills(sess: &Session, sub_id: String, cwds: Vec<PathBuf>, force_reload: bool) {
    let cwds = if cwds.is_empty() {
        let state = sess.state.lock().await;
        vec![state.session_configuration.cwd.to_path_buf()]
    } else {
        cwds
    };

    let skills_manager = &sess.services.skills_manager;
    let plugins_manager = &sess.services.plugins_manager;
    let config = sess.get_config().await;
    let praxis_home = sess.praxis_home().await;
    let mut skills = Vec::new();
    let empty_cli_overrides: &[(String, toml::Value)] = &[];
    for cwd in cwds {
        let cwd_abs = match AbsolutePathBuf::try_from(cwd.as_path()) {
            Ok(path) => path,
            Err(err) => {
                let message = err.to_string();
                let cwd_for_entry = cwd.clone();
                skills.push(SkillsListEntry {
                    cwd: cwd_for_entry.clone(),
                    skills: Vec::new(),
                    errors: super::errors_to_info(&[SkillError {
                        path: cwd_for_entry,
                        message,
                    }]),
                });
                continue;
            }
        };
        let config_layer_stack = match load_config_layers_state(
            &praxis_home,
            Some(cwd_abs),
            empty_cli_overrides,
            LoaderOverrides::default(),
            CloudConfigBundleLoader::default(),
        )
        .await
        {
            Ok(config_layer_stack) => config_layer_stack,
            Err(err) => {
                let message = err.to_string();
                let cwd_for_entry = cwd.clone();
                skills.push(SkillsListEntry {
                    cwd: cwd_for_entry.clone(),
                    skills: Vec::new(),
                    errors: super::errors_to_info(&[SkillError {
                        path: cwd_for_entry,
                        message,
                    }]),
                });
                continue;
            }
        };
        let effective_skill_roots = plugins_manager.effective_skill_roots_for_layer_stack(
            &config_layer_stack,
            config.features.enabled(Feature::Plugins),
        );
        let skills_input = crate::SkillsLoadInput::new(
            cwd.clone(),
            effective_skill_roots,
            config_layer_stack,
            config.bundled_skills_enabled(),
        );
        let outcome = skills_manager
            .skills_for_cwd(&skills_input, force_reload)
            .await;
        let errors = super::errors_to_info(&outcome.errors);
        let skills_metadata = super::skills_to_info(&outcome.skills, &outcome.disabled_paths);
        skills.push(SkillsListEntry {
            cwd,
            skills: skills_metadata,
            errors,
        });
    }

    let event = Event {
        id: sub_id,
        msg: EventMsg::ListSkillsResponse(ListSkillsResponseEvent { skills }),
    };
    sess.send_event_raw(event).await;
}

pub async fn undo(sess: &Arc<Session>, sub_id: String) {
    let turn_context = sess.new_default_turn_with_sub_id(sub_id).await;
    sess.spawn_task(turn_context, Vec::new(), UndoTask::new())
        .await;
}

pub async fn compact(sess: &Arc<Session>, sub_id: String) {
    let turn_context = sess.new_default_turn_with_sub_id(sub_id).await;

    sess.spawn_task(
        Arc::clone(&turn_context),
        vec![UserInput::Text {
            text: turn_context.compact_prompt().to_string(),
            // Compaction prompt is synthesized; no UI element ranges to preserve.
            text_elements: Vec::new(),
        }],
        CompactTask,
    )
    .await;
}

pub async fn drop_memories(sess: &Arc<Session>, config: &Arc<Config>, sub_id: String) {
    let mut errors = Vec::new();

    if let Some(state_db) = sess.services.state_db.as_deref() {
        if let Err(err) = state_db.clear_memory_data().await {
            errors.push(format!("failed clearing memory rows from state db: {err}"));
        }
    } else {
        errors.push("state db unavailable; memory rows were not cleared".to_string());
    }

    let memory_root = crate::memories::memory_root(&config.praxis_home);
    if let Err(err) = crate::memories::clear_memory_root_contents(&memory_root).await {
        errors.push(format!(
            "failed clearing memory directory {}: {err}",
            memory_root.display()
        ));
    }

    if errors.is_empty() {
        sess.raw_event_emitter(sub_id)
            .warning(format!(
                "Dropped memories at {} and cleared memory rows from state db.",
                memory_root.display()
            ))
            .await;
        return;
    }

    sess.raw_event_emitter(sub_id)
        .error(
            format!("Memory drop completed with errors: {}", errors.join("; ")),
            Some(CodexErrorInfo::Other),
        )
        .await;
}

pub async fn update_memories(sess: &Arc<Session>, config: &Arc<Config>, sub_id: String) {
    let session_source = {
        let state = sess.state.lock().await;
        state.session_configuration.session_source.clone()
    };

    crate::memories::start_memories_startup_task(sess, Arc::clone(config), &session_source);

    sess.raw_event_emitter(sub_id)
        .warning("Memory update triggered.")
        .await;
}

pub async fn thread_rollback(sess: &Arc<Session>, sub_id: String, num_turns: u32) {
    if num_turns == 0 {
        sess.raw_event_emitter(sub_id)
            .error(
                "num_turns must be >= 1",
                Some(CodexErrorInfo::ThreadRollbackFailed),
            )
            .await;
        return;
    }

    let has_active_turn = { sess.active_turn.lock().await.is_some() };
    if has_active_turn {
        sess.raw_event_emitter(sub_id)
            .error(
                "Cannot rollback while a turn is in progress.",
                Some(CodexErrorInfo::ThreadRollbackFailed),
            )
            .await;
        return;
    }

    let turn_context = sess.new_default_turn_with_sub_id(sub_id).await;
    let rollout_path = {
        let recorder = {
            let guard = sess.services.rollout.lock().await;
            guard.clone()
        };
        let Some(recorder) = recorder else {
            sess.raw_event_emitter(turn_context.sub_id.clone())
                .error(
                    "thread rollback requires a persisted rollout path",
                    Some(CodexErrorInfo::ThreadRollbackFailed),
                )
                .await;
            return;
        };
        recorder.rollout_path().to_path_buf()
    };
    if let Some(recorder) = {
        let guard = sess.services.rollout.lock().await;
        guard.clone()
    } && let Err(err) = recorder.flush().await
    {
        sess.raw_event_emitter(turn_context.sub_id.clone())
            .error(
                format!(
                    "failed to flush rollout `{}` for rollback replay: {err}",
                    rollout_path.display()
                ),
                Some(CodexErrorInfo::ThreadRollbackFailed),
            )
            .await;
        return;
    }

    let initial_history = match RolloutRecorder::get_rollout_history(rollout_path.as_path()).await {
        Ok(history) => history,
        Err(err) => {
            sess.raw_event_emitter(turn_context.sub_id.clone())
                .error(
                    format!(
                        "failed to load rollout `{}` for rollback replay: {err}",
                        rollout_path.display()
                    ),
                    Some(CodexErrorInfo::ThreadRollbackFailed),
                )
                .await;
            return;
        }
    };

    let rollback_event = ThreadRolledBackEvent { num_turns };
    let rollback_msg = EventMsg::ThreadRolledBack(rollback_event.clone());
    let replay_items = initial_history
        .get_rollout_items()
        .into_iter()
        .chain(std::iter::once(RolloutItem::EventMsg(rollback_msg.clone())))
        .collect::<Vec<_>>();
    sess.persist_rollout_items(&[RolloutItem::EventMsg(rollback_msg.clone())])
        .await;
    sess.flush_rollout().await;
    sess.apply_rollout_reconstruction(turn_context.as_ref(), replay_items.as_slice())
        .await;
    sess.recompute_token_usage(turn_context.as_ref()).await;

    sess.deliver_event_raw(Event {
        id: turn_context.sub_id.clone(),
        msg: rollback_msg,
    })
    .await;
}

/// Persists the thread name, updates in-memory state, and emits `ThreadNameUpdated` on success.
pub async fn set_thread_name(sess: &Arc<Session>, sub_id: String, name: String) {
    let Some(name) = crate::util::normalize_thread_name(&name) else {
        sess.raw_event_emitter(sub_id)
            .error(
                "Thread name cannot be empty.",
                Some(CodexErrorInfo::BadRequest),
            )
            .await;
        return;
    };

    let persistence_enabled = {
        let rollout = sess.services.rollout.lock().await;
        rollout.is_some()
    };
    if !persistence_enabled {
        sess.raw_event_emitter(sub_id)
            .error(
                "Session persistence is disabled; cannot rename thread.",
                Some(CodexErrorInfo::Other),
            )
            .await;
        return;
    };

    if let Err(e) = praxis_rollout::ThreadNameWriter::new(sess.services.state_db.as_deref())
        .write_name(sess.conversation_id, &name)
        .await
    {
        sess.raw_event_emitter(sub_id)
            .error(
                format!("Failed to set thread name: {e}"),
                Some(CodexErrorInfo::Other),
            )
            .await;
        return;
    }

    {
        let mut state = sess.state.lock().await;
        state.session_configuration.thread_name = Some(name.clone());
    }

    sess.send_event_raw(Event {
        id: sub_id,
        msg: EventMsg::ThreadNameUpdated(ThreadNameUpdatedEvent {
            thread_id: sess.conversation_id,
            thread_name: Some(name),
        }),
    })
    .await;
}

pub async fn shutdown(sess: &Arc<Session>, sub_id: String) -> bool {
    sess.abort_all_tasks(TurnAbortReason::Interrupted).await;
    let _ = sess.conversation.shutdown().await;
    sess.services
        .unified_exec_manager
        .terminate_all_processes()
        .await;
    sess.guardian_review_session.shutdown().await;
    info!("Shutting down Praxis instance");
    let history = sess.clone_history().await;
    let turn_count = history
        .raw_items()
        .iter()
        .filter(|item| is_user_turn_boundary(item))
        .count();
    sess.services.session_telemetry.counter(
        "codex.conversation.turn.count",
        i64::try_from(turn_count).unwrap_or(0),
        &[],
    );

    // Gracefully flush and shutdown rollout recorder on session end so tests
    // that inspect the rollout file do not race with the background writer.
    let recorder_opt = {
        let mut guard = sess.services.rollout.lock().await;
        guard.take()
    };
    if let Some(rec) = recorder_opt
        && let Err(e) = rec.shutdown().await
    {
        warn!("failed to shutdown rollout recorder: {e}");
        sess.raw_event_emitter(sub_id.clone())
            .error(
                "Failed to shutdown rollout recorder",
                Some(CodexErrorInfo::Other),
            )
            .await;
    }

    let event = Event {
        id: sub_id,
        msg: EventMsg::ShutdownComplete,
    };
    sess.send_event_raw(event).await;
    true
}

pub async fn review(
    sess: &Arc<Session>,
    config: &Arc<Config>,
    sub_id: String,
    review_request: ReviewRequest,
) {
    let turn_context = sess.new_default_turn_with_sub_id(sub_id.clone()).await;
    sess.maybe_emit_unknown_model_warning_for_turn(turn_context.as_ref())
        .await;
    sess.refresh_mcp_servers_if_requested(&turn_context).await;
    match resolve_review_request(review_request, turn_context.cwd.as_path()) {
        Ok(resolved) => {
            spawn_review_thread(
                Arc::clone(sess),
                Arc::clone(config),
                turn_context.clone(),
                sub_id,
                resolved,
            )
            .await;
        }
        Err(err) => {
            sess.turn_event_emitter(&turn_context)
                .error(err.to_string(), Some(CodexErrorInfo::Other))
                .await;
        }
    }
}
