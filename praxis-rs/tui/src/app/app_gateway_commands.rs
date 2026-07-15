use super::App;
use super::thread_event_store::ThreadEventStore;
use crate::app_command::AppCommand;
use crate::app_command::AppCommandView;
use crate::app_gateway_session::AppGatewaySession;
use crate::tui;
use color_eyre::eyre::Result;
use praxis_app_gateway_client::TypedRequestError;
use praxis_app_gateway_protocol::PraxisErrorInfo as AppGatewayPraxisErrorInfo;
use praxis_app_gateway_protocol::TurnError as AppGatewayTurnError;
use praxis_protocol::ThreadId;
use praxis_protocol::protocol::Op;

impl App {
    pub(super) async fn submit_active_thread_op(
        &mut self,
        app_gateway: &mut AppGatewaySession,
        op: AppCommand,
    ) -> Result<()> {
        let Some(thread_id) = self.active_thread_id.or(self.chat_widget.thread_id()) else {
            self.chat_widget
                .add_error_message("No active thread is available.".to_string());
            return Ok(());
        };

        self.submit_thread_op(app_gateway, thread_id, op).await
    }

    pub(super) async fn submit_active_thread_op_or_start(
        &mut self,
        tui: &mut tui::Tui,
        app_gateway: &mut AppGatewaySession,
        op: AppCommand,
    ) -> Result<()> {
        if op.is_user_turn()
            && self.active_thread_id.is_none()
            && self.chat_widget.thread_id().is_none()
        {
            self.start_fresh_session_with_summary_hint(tui, app_gateway)
                .await;
        }
        self.submit_active_thread_op(app_gateway, op).await
    }

    pub(super) async fn submit_thread_op(
        &mut self,
        app_gateway: &mut AppGatewaySession,
        thread_id: ThreadId,
        op: AppCommand,
    ) -> Result<()> {
        crate::session_log::log_outbound_op(&op);

        if self
            .try_submit_history_op_via_app_gateway(app_gateway, thread_id, &op)
            .await?
        {
            return Ok(());
        }

        if self
            .try_resolve_app_gateway_request(app_gateway, thread_id, &op)
            .await?
        {
            return Ok(());
        }

        if self
            .try_submit_active_thread_op_via_app_gateway(app_gateway, thread_id, &op)
            .await?
        {
            if ThreadEventStore::op_can_change_pending_replay_state(&op) {
                self.note_thread_outbound_op(thread_id, &op).await;
                self.refresh_pending_thread_approvals().await;
            }
            return Ok(());
        }

        self.chat_widget
            .add_error_message(format!("Not available in TUI yet for thread {thread_id}."));
        Ok(())
    }

    async fn try_submit_history_op_via_app_gateway(
        &mut self,
        app_gateway: &mut AppGatewaySession,
        thread_id: ThreadId,
        op: &AppCommand,
    ) -> Result<bool> {
        match op.view() {
            AppCommandView::Other(Op::AddToHistory { text }) => {
                app_gateway
                    .thread_history_append(thread_id, text.clone())
                    .await?;
                Ok(true)
            }
            AppCommandView::Other(Op::GetHistoryEntryRequest { offset, log_id }) => {
                app_gateway
                    .thread_history_entry_get(thread_id, *offset, *log_id)
                    .await?;
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    async fn try_submit_active_thread_op_via_app_gateway(
        &mut self,
        app_gateway: &mut AppGatewaySession,
        thread_id: ThreadId,
        op: &AppCommand,
    ) -> Result<bool> {
        match op.view() {
            AppCommandView::Interrupt => {
                let Some(turn_id) = self.active_turn_id_for_thread(thread_id).await else {
                    return Ok(true);
                };
                app_gateway.turn_interrupt(thread_id, turn_id).await?;
                Ok(true)
            }
            AppCommandView::UserTurn {
                items,
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
                collaboration_mode,
                personality,
            } => {
                let mut should_start_turn = true;
                if let Some(turn_id) = self.active_turn_id_for_thread(thread_id).await {
                    match app_gateway
                        .turn_steer(thread_id, turn_id, items.to_vec())
                        .await
                    {
                        Ok(_) => return Ok(true),
                        Err(error) => {
                            if let Some(turn_error) = active_turn_not_steerable_turn_error(&error) {
                                if !self.chat_widget.enqueue_rejected_steer() {
                                    self.chat_widget.add_error_message(turn_error.message);
                                }
                                return Ok(true);
                            } else if active_turn_missing_steer_error(&error) {
                                if let Some(channel) = self.thread_event_channels.get(&thread_id) {
                                    let mut store = channel.store.lock().await;
                                    store.clear_active_turn_id();
                                }
                                should_start_turn = true;
                            } else {
                                return Err(error.into());
                            }
                        }
                    }
                }
                if should_start_turn {
                    app_gateway
                        .turn_start(
                            thread_id,
                            items.to_vec(),
                            cwd.clone(),
                            approval_policy,
                            approvals_reviewer
                                .unwrap_or(self.chat_widget.config_ref().approvals_reviewer),
                            sandbox_policy.clone(),
                            model_provider.clone(),
                            model.to_string(),
                            effort,
                            *summary,
                            *service_tier,
                            collaboration_mode.clone(),
                            *personality,
                            final_output_json_schema.clone(),
                        )
                        .await?;
                }
                Ok(true)
            }
            AppCommandView::ListSkills { cwds, force_reload } => {
                let response = app_gateway
                    .skills_list(praxis_app_gateway_protocol::SkillsListParams {
                        cwds: cwds.to_vec(),
                        force_reload,
                        per_cwd_extra_user_roots: None,
                    })
                    .await?;
                self.handle_skills_list_response(response);
                Ok(true)
            }
            AppCommandView::Compact => {
                app_gateway.thread_compact_start(thread_id).await?;
                Ok(true)
            }
            AppCommandView::SetThreadName { name } => {
                app_gateway
                    .thread_set_name(thread_id, name.to_string())
                    .await?;
                Ok(true)
            }
            AppCommandView::ThreadRollback { num_turns } => {
                let response = match app_gateway.thread_rollback(thread_id, num_turns).await {
                    Ok(response) => response,
                    Err(err) => {
                        self.handle_backtrack_rollback_failed();
                        return Err(err);
                    }
                };
                self.handle_thread_rollback_response(thread_id, num_turns, &response)
                    .await;
                Ok(true)
            }
            AppCommandView::Review { review_request } => {
                app_gateway
                    .review_start(thread_id, review_request.clone())
                    .await?;
                Ok(true)
            }
            AppCommandView::CleanBackgroundTerminals => {
                app_gateway
                    .thread_background_terminals_clean(thread_id)
                    .await?;
                Ok(true)
            }
            AppCommandView::RealtimeConversationStart(params) => {
                app_gateway
                    .thread_realtime_start(thread_id, params.clone())
                    .await?;
                Ok(true)
            }
            AppCommandView::RealtimeConversationAudio(params) => {
                app_gateway
                    .thread_realtime_audio(thread_id, params.clone())
                    .await?;
                Ok(true)
            }
            AppCommandView::RealtimeConversationText(params) => {
                app_gateway
                    .thread_realtime_text(thread_id, params.clone())
                    .await?;
                Ok(true)
            }
            AppCommandView::RealtimeConversationClose => {
                app_gateway.thread_realtime_stop(thread_id).await?;
                Ok(true)
            }
            AppCommandView::RunUserShellCommand { command } => {
                app_gateway
                    .thread_shell_command(thread_id, command.to_string())
                    .await?;
                Ok(true)
            }
            AppCommandView::ReloadUserConfig => {
                app_gateway.reload_user_config().await?;
                Ok(true)
            }
            AppCommandView::OverrideTurnContext { .. } => Ok(true),
            _ => Ok(false),
        }
    }

    async fn try_resolve_app_gateway_request(
        &mut self,
        app_gateway: &AppGatewaySession,
        thread_id: ThreadId,
        op: &AppCommand,
    ) -> Result<bool> {
        let Some(resolution) = self
            .pending_app_gateway_requests
            .take_resolution(op)
            .map_err(|err| color_eyre::eyre::eyre!(err))?
        else {
            return Ok(false);
        };

        match app_gateway
            .resolve_server_request(resolution.request_id, resolution.result)
            .await
        {
            Ok(()) => {
                if ThreadEventStore::op_can_change_pending_replay_state(op) {
                    self.note_thread_outbound_op(thread_id, op).await;
                    self.refresh_pending_thread_approvals().await;
                }
                Ok(true)
            }
            Err(err) => {
                self.chat_widget.add_error_message(format!(
                    "Failed to resolve app-gateway request for thread {thread_id}: {err}"
                ));
                Ok(false)
            }
        }
    }
}

fn active_turn_not_steerable_turn_error(error: &TypedRequestError) -> Option<AppGatewayTurnError> {
    let TypedRequestError::Server { source, .. } = error else {
        return None;
    };
    let turn_error: AppGatewayTurnError = serde_json::from_value(source.data.clone()?).ok()?;
    matches!(
        turn_error.praxis_error_info,
        Some(AppGatewayPraxisErrorInfo::ActiveTurnNotSteerable { .. })
    )
    .then_some(turn_error)
}

fn active_turn_missing_steer_error(error: &TypedRequestError) -> bool {
    let TypedRequestError::Server { source, .. } = error else {
        return false;
    };
    let Some(data) = source.data.clone() else {
        return false;
    };
    let Ok(turn_error) = serde_json::from_value::<AppGatewayTurnError>(data) else {
        return false;
    };
    matches!(
        turn_error.praxis_error_info,
        Some(AppGatewayPraxisErrorInfo::NoActiveTurnToSteer)
    )
}
