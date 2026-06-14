use super::*;

pub(super) async fn main_agent_loop(
    sess: Arc<Session>,
    config: Arc<Config>,
    rx_sub: Receiver<Submission>,
) {
    // To break out of this loop, send Op::Shutdown.
    while let Ok(sub) = rx_sub.recv().await {
        debug!(?sub, "Submission");
        let dispatch_span = submission_dispatch_span(&sub);
        let should_exit = dispatch_submission(&sess, &config, sub)
            .instrument(dispatch_span)
            .await;
        if should_exit {
            break;
        }
    }
    // Also drain cached guardian state if the submission loop exits because
    // the channel closed without receiving an explicit shutdown op.
    sess.guardian_review_session.shutdown().await;
    debug!("Agent loop exited");
}

async fn dispatch_submission(sess: &Arc<Session>, config: &Arc<Config>, sub: Submission) -> bool {
    dispatch_op(sess, config, sub.id, sub.op).await
}

async fn dispatch_op(sess: &Arc<Session>, config: &Arc<Config>, sub_id: String, op: Op) -> bool {
    match op {
        Op::Interrupt => {
            handlers::interrupt(sess).await;
            false
        }
        Op::CleanBackgroundTerminals => {
            handlers::clean_background_terminals(sess).await;
            false
        }
        Op::RealtimeConversationStart(params) => {
            if let Err(err) = handle_realtime_conversation_start(sess, sub_id.clone(), params).await
            {
                send_submission_error(sess, sub_id, err.to_string()).await;
            }
            false
        }
        Op::RealtimeConversationAudio(params) => {
            handle_realtime_conversation_audio(sess, sub_id, params).await;
            false
        }
        Op::RealtimeConversationText(params) => {
            handle_realtime_conversation_text(sess, sub_id, params).await;
            false
        }
        Op::RealtimeConversationClose => {
            handle_realtime_conversation_close(sess, sub_id).await;
            false
        }
        Op::OverrideTurnContext {
            cwd,
            approval_policy,
            approvals_reviewer,
            sandbox_policy,
            windows_sandbox_level,
            model_provider,
            model,
            effort,
            summary,
            service_tier,
            collaboration_mode,
            personality,
        } => {
            handle_override_turn_context(
                sess,
                sub_id,
                OverrideTurnContextUpdate {
                    cwd,
                    approval_policy,
                    approvals_reviewer,
                    sandbox_policy,
                    windows_sandbox_level,
                    model_provider,
                    model,
                    effort,
                    summary,
                    service_tier,
                    collaboration_mode,
                    personality,
                },
            )
            .await;
            false
        }
        op @ (Op::UserInput { .. } | Op::UserTurn { .. }) => {
            handlers::user_input_or_turn(sess, sub_id, op).await;
            false
        }
        Op::InterAgentCommunication { communication } => {
            handlers::inter_agent_communication(sess, sub_id, communication).await;
            false
        }
        Op::ExecApproval {
            id: approval_id,
            turn_id,
            decision,
        } => {
            handlers::exec_approval(sess, approval_id, turn_id, decision).await;
            false
        }
        Op::PatchApproval { id, decision } => {
            handlers::patch_approval(sess, id, decision).await;
            false
        }
        Op::UserInputAnswer { id, response } => {
            handlers::request_user_input_response(sess, id, response).await;
            false
        }
        Op::RequestPermissionsResponse { id, response } => {
            handlers::request_permissions_response(sess, id, response).await;
            false
        }
        Op::DynamicToolResponse { id, response } => {
            handlers::dynamic_tool_response(sess, id, response).await;
            false
        }
        Op::AddToHistory { text } => {
            handlers::add_to_history(sess, config, text).await;
            false
        }
        Op::GetHistoryEntryRequest { offset, log_id } => {
            handlers::get_history_entry_request(sess, config, sub_id, offset, log_id).await;
            false
        }
        Op::ListMcpTools => {
            handlers::list_mcp_tools(sess, config, sub_id).await;
            false
        }
        Op::RefreshMcpServers { config } => {
            handlers::refresh_mcp_servers(sess, config).await;
            false
        }
        Op::ReloadUserConfig => {
            handlers::reload_user_config(sess).await;
            false
        }
        Op::ListSkills { cwds, force_reload } => {
            handlers::list_skills(sess, sub_id, cwds, force_reload).await;
            false
        }
        Op::Undo => {
            handlers::undo(sess, sub_id).await;
            false
        }
        Op::Compact => {
            handlers::compact(sess, sub_id).await;
            false
        }
        Op::DropMemories => {
            handlers::drop_memories(sess, config, sub_id).await;
            false
        }
        Op::UpdateMemories => {
            handlers::update_memories(sess, config, sub_id).await;
            false
        }
        Op::ThreadRollback { num_turns } => {
            handlers::thread_rollback(sess, sub_id, num_turns).await;
            false
        }
        Op::SetThreadName { name } => {
            handlers::set_thread_name(sess, sub_id, name).await;
            false
        }
        Op::RunUserShellCommand { command } => {
            handlers::run_user_shell_command(sess, sub_id, command).await;
            false
        }
        Op::ResolveElicitation {
            server_name,
            request_id,
            decision,
            content,
            meta,
        } => {
            handlers::resolve_elicitation(sess, server_name, request_id, decision, content, meta)
                .await;
            false
        }
        Op::Shutdown => handlers::shutdown(sess, sub_id).await,
        Op::Review { review_request } => {
            handlers::review(sess, config, sub_id, review_request).await;
            false
        }
        _ => false, // Ignore unknown ops; enum is non_exhaustive to allow extensions.
    }
}

struct OverrideTurnContextUpdate {
    cwd: Option<PathBuf>,
    approval_policy: Option<AskForApproval>,
    approvals_reviewer: Option<ApprovalsReviewer>,
    sandbox_policy: Option<SandboxPolicy>,
    windows_sandbox_level: Option<WindowsSandboxLevel>,
    model_provider: Option<String>,
    model: Option<String>,
    effort: Option<Option<ReasoningEffortConfig>>,
    summary: Option<ReasoningSummaryConfig>,
    service_tier: Option<Option<ServiceTier>>,
    collaboration_mode: Option<CollaborationMode>,
    personality: Option<Personality>,
}

async fn handle_override_turn_context(
    sess: &Arc<Session>,
    sub_id: String,
    update: OverrideTurnContextUpdate,
) {
    let collaboration_mode = if let Some(collab_mode) = update.collaboration_mode {
        collab_mode
    } else {
        let state = sess.state.lock().await;
        state.session_configuration.collaboration_mode.with_updates(
            update.model.clone(),
            update.effort,
            /*developer_instructions*/ None,
        )
    };
    handlers::override_turn_context(
        sess,
        sub_id,
        SessionSettingsUpdate {
            cwd: update.cwd,
            approval_policy: update.approval_policy,
            approvals_reviewer: update.approvals_reviewer,
            sandbox_policy: update.sandbox_policy,
            windows_sandbox_level: update.windows_sandbox_level,
            model_provider: update.model_provider,
            collaboration_mode: Some(collaboration_mode),
            reasoning_summary: update.summary,
            service_tier: update.service_tier,
            personality: update.personality,
            ..Default::default()
        },
    )
    .await;
}

async fn send_submission_error(sess: &Arc<Session>, sub_id: String, message: String) {
    sess.raw_event_emitter(sub_id)
        .error(message, Some(CodexErrorInfo::Other))
        .await;
}

pub(super) fn submission_dispatch_span(sub: &Submission) -> tracing::Span {
    let op_name = sub.op.kind();
    let span_name = format!("op.dispatch.{op_name}");
    let dispatch_span = match &sub.op {
        Op::RealtimeConversationAudio(_) => {
            debug_span!(
                "submission_dispatch",
                otel.name = span_name.as_str(),
                submission.id = sub.id.as_str(),
                codex.op = op_name
            )
        }
        _ => info_span!(
            "submission_dispatch",
            otel.name = span_name.as_str(),
            submission.id = sub.id.as_str(),
            codex.op = op_name
        ),
    };
    if let Some(trace) = sub.trace.as_ref()
        && !set_parent_from_w3c_trace_context(&dispatch_span, trace)
    {
        warn!(
            submission.id = sub.id.as_str(),
            "ignoring invalid submission trace carrier"
        );
    }
    dispatch_span
}
