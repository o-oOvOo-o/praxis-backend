use super::*;

impl AppGatewaySession {
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn turn_start(
        &mut self,
        thread_id: ThreadId,
        items: Vec<praxis_protocol::user_input::UserInput>,
        cwd: PathBuf,
        approval_policy: AskForApproval,
        approvals_reviewer: praxis_protocol::config_types::ApprovalsReviewer,
        sandbox_policy: SandboxPolicy,
        model_provider: Option<String>,
        model: String,
        effort: Option<praxis_protocol::openai_models::ReasoningEffort>,
        summary: Option<praxis_protocol::config_types::ReasoningSummary>,
        service_tier: Option<Option<praxis_protocol::config_types::ServiceTier>>,
        collaboration_mode: Option<praxis_protocol::config_types::CollaborationMode>,
        personality: Option<praxis_protocol::config_types::Personality>,
        output_schema: Option<serde_json::Value>,
    ) -> Result<TurnStartResponse> {
        let request_id = self.next_request_id();
        self.client
            .request_typed(ClientRequest::TurnStart {
                request_id,
                params: TurnStartParams {
                    thread_id: thread_id.to_string(),
                    input: items.into_iter().map(Into::into).collect(),
                    cwd: Some(cwd),
                    approval_policy: Some(approval_policy.into()),
                    approvals_reviewer: Some(approvals_reviewer.into()),
                    sandbox_policy: Some(sandbox_policy.into()),
                    model_provider,
                    model: Some(model),
                    service_tier,
                    effort,
                    summary,
                    personality,
                    output_schema,
                    collaboration_mode,
                },
            })
            .await
            .wrap_err("turn/start failed in TUI")
    }

    pub(crate) async fn turn_interrupt(
        &mut self,
        thread_id: ThreadId,
        turn_id: String,
    ) -> Result<()> {
        let request_id = self.next_request_id();
        let _: TurnInterruptResponse = self
            .client
            .request_typed(ClientRequest::TurnInterrupt {
                request_id,
                params: TurnInterruptParams {
                    thread_id: thread_id.to_string(),
                    turn_id,
                },
            })
            .await
            .wrap_err("turn/interrupt failed in TUI")?;
        Ok(())
    }

    pub(crate) async fn turn_steer(
        &mut self,
        thread_id: ThreadId,
        turn_id: String,
        items: Vec<praxis_protocol::user_input::UserInput>,
    ) -> std::result::Result<TurnSteerResponse, TypedRequestError> {
        let request_id = self.next_request_id();
        self.client
            .request_typed(ClientRequest::TurnSteer {
                request_id,
                params: TurnSteerParams {
                    thread_id: thread_id.to_string(),
                    input: items.into_iter().map(Into::into).collect(),
                    expected_turn_id: turn_id,
                },
            })
            .await
    }

    pub(crate) async fn thread_set_name(
        &mut self,
        thread_id: ThreadId,
        name: String,
    ) -> Result<()> {
        let request_id = self.next_request_id();
        let _: ThreadSetNameResponse = self
            .client
            .request_typed(ClientRequest::ThreadSetName {
                request_id,
                params: ThreadSetNameParams {
                    thread_id: thread_id.to_string(),
                    name,
                },
            })
            .await
            .wrap_err("thread/name/set failed in TUI")?;
        Ok(())
    }

    pub(crate) async fn thread_regenerate_name(&mut self, thread_id: ThreadId) -> Result<String> {
        let request_id = self.next_request_id();
        let response: ThreadRegenerateNameResponse = self
            .client
            .request_typed(ClientRequest::ThreadRegenerateName {
                request_id,
                params: ThreadRegenerateNameParams {
                    thread_id: thread_id.to_string(),
                },
            })
            .await
            .wrap_err("thread/name/regenerate failed in TUI")?;
        Ok(response.thread_name)
    }

    pub(crate) async fn thread_archive(&mut self, thread_id: ThreadId) -> Result<()> {
        let request_id = self.next_request_id();
        let _: ThreadArchiveResponse = self
            .client
            .request_typed(ClientRequest::ThreadArchive {
                request_id,
                params: ThreadArchiveParams {
                    thread_id: thread_id.to_string(),
                },
            })
            .await
            .wrap_err("thread/archive failed in TUI")?;
        Ok(())
    }

    pub(crate) async fn thread_delete(&mut self, thread_id: ThreadId) -> Result<()> {
        let request_id = self.next_request_id();
        let _: ThreadDeleteResponse = self
            .client
            .request_typed(ClientRequest::ThreadDelete {
                request_id,
                params: ThreadDeleteParams {
                    thread_id: thread_id.to_string(),
                },
            })
            .await
            .wrap_err("thread/delete failed in TUI")?;
        Ok(())
    }

    pub(crate) async fn thread_control_release(
        &mut self,
        thread_id: ThreadId,
    ) -> Result<Option<ThreadControlState>> {
        let request_id = self.next_request_id();
        let response: ThreadControlReleaseResponse = self
            .client
            .request_typed(ClientRequest::ThreadControlRelease {
                request_id,
                params: ThreadControlReleaseParams {
                    thread_id: thread_id.to_string(),
                    controller: None,
                },
            })
            .await
            .wrap_err("thread/control/release failed in TUI")?;
        Ok(response.previous_control_state)
    }

    pub(crate) async fn thread_set_selfwork_plan_path(
        &mut self,
        thread_id: ThreadId,
        selfwork_plan_path: Option<PathBuf>,
    ) -> Result<()> {
        let request_id = self.next_request_id();
        let _: ThreadMetadataUpdateResponse = self
            .client
            .request_typed(ClientRequest::ThreadMetadataUpdate {
                request_id,
                params: ThreadMetadataUpdateParams {
                    thread_id: thread_id.to_string(),
                    git_info: None,
                    selfwork_plan_path: Some(selfwork_plan_path),
                },
            })
            .await
            .wrap_err("thread/metadata/update failed in TUI")?;
        Ok(())
    }

    pub(crate) async fn thread_unsubscribe(&mut self, thread_id: ThreadId) -> Result<()> {
        let request_id = self.next_request_id();
        let _: ThreadUnsubscribeResponse = self
            .client
            .request_typed(ClientRequest::ThreadUnsubscribe {
                request_id,
                params: ThreadUnsubscribeParams {
                    thread_id: thread_id.to_string(),
                },
            })
            .await
            .wrap_err("thread/unsubscribe failed in TUI")?;
        Ok(())
    }

    pub(crate) async fn thread_compact_start(&mut self, thread_id: ThreadId) -> Result<()> {
        let request_id = self.next_request_id();
        let _: ThreadCompactStartResponse = self
            .client
            .request_typed(ClientRequest::ThreadCompactStart {
                request_id,
                params: ThreadCompactStartParams {
                    thread_id: thread_id.to_string(),
                },
            })
            .await
            .wrap_err("thread/compact/start failed in TUI")?;
        Ok(())
    }

    pub(crate) async fn thread_shell_command(
        &mut self,
        thread_id: ThreadId,
        command: String,
    ) -> Result<()> {
        let request_id = self.next_request_id();
        let _: ThreadShellCommandResponse = self
            .client
            .request_typed(ClientRequest::ThreadShellCommand {
                request_id,
                params: ThreadShellCommandParams {
                    thread_id: thread_id.to_string(),
                    command,
                },
            })
            .await
            .wrap_err("thread/shellCommand failed in TUI")?;
        Ok(())
    }

    pub(crate) async fn thread_history_append(
        &mut self,
        thread_id: ThreadId,
        text: String,
    ) -> Result<()> {
        let request_id = self.next_request_id();
        let _: ThreadHistoryAppendResponse = self
            .client
            .request_typed(ClientRequest::ThreadHistoryAppend {
                request_id,
                params: ThreadHistoryAppendParams {
                    thread_id: thread_id.to_string(),
                    text,
                },
            })
            .await
            .wrap_err("thread/history/append failed in TUI")?;
        Ok(())
    }

    pub(crate) async fn thread_history_entry_get(
        &mut self,
        thread_id: ThreadId,
        offset: usize,
        log_id: u64,
    ) -> Result<()> {
        let request_id = self.next_request_id();
        let _: ThreadHistoryEntryGetResponse = self
            .client
            .request_typed(ClientRequest::ThreadHistoryEntryGet {
                request_id,
                params: ThreadHistoryEntryGetParams {
                    thread_id: thread_id.to_string(),
                    offset,
                    log_id,
                },
            })
            .await
            .wrap_err("thread/history/get failed in TUI")?;
        Ok(())
    }

    pub(crate) async fn thread_background_terminals_clean(
        &mut self,
        thread_id: ThreadId,
    ) -> Result<()> {
        let request_id = self.next_request_id();
        let _: ThreadBackgroundTerminalsCleanResponse = self
            .client
            .request_typed(ClientRequest::ThreadBackgroundTerminalsClean {
                request_id,
                params: ThreadBackgroundTerminalsCleanParams {
                    thread_id: thread_id.to_string(),
                },
            })
            .await
            .wrap_err("thread/backgroundTerminals/clean failed in TUI")?;
        Ok(())
    }

    pub(crate) async fn thread_rollback(
        &mut self,
        thread_id: ThreadId,
        num_turns: u32,
    ) -> Result<ThreadRollbackResponse> {
        let request_id = self.next_request_id();
        self.client
            .request_typed(ClientRequest::ThreadRollback {
                request_id,
                params: ThreadRollbackParams {
                    thread_id: thread_id.to_string(),
                    num_turns,
                },
            })
            .await
            .wrap_err("thread/rollback failed in TUI")
    }
}
