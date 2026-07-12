use super::*;

impl McpProcess {
    pub async fn send_get_account_rate_limits_request(&mut self) -> anyhow::Result<i64> {
        self.send_request("account/rateLimits/read", /*params*/ None)
            .await
    }

    /// Send an `account/read` JSON-RPC request.
    pub async fn send_get_account_request(
        &mut self,
        params: GetAccountParams,
    ) -> anyhow::Result<i64> {
        let params = Some(serde_json::to_value(params)?);
        self.send_request("account/read", params).await
    }

    /// Send an `account/login/start` JSON-RPC request with ChatGPT auth tokens.
    pub async fn send_chatgpt_auth_tokens_login_request(
        &mut self,
        access_token: String,
        chatgpt_account_id: String,
        chatgpt_plan_type: Option<String>,
    ) -> anyhow::Result<i64> {
        let params = LoginAccountParams::ChatgptAuthTokens {
            access_token,
            chatgpt_account_id,
            chatgpt_plan_type,
        };
        let params = Some(serde_json::to_value(params)?);
        self.send_request("account/login/start", params).await
    }

    /// Send a `feedback/upload` JSON-RPC request.
    pub async fn send_feedback_upload_request(
        &mut self,
        params: FeedbackUploadParams,
    ) -> anyhow::Result<i64> {
        let params = Some(serde_json::to_value(params)?);
        self.send_request("feedback/upload", params).await
    }

    /// Send a `thread/start` JSON-RPC request.
    pub async fn send_thread_start_request(
        &mut self,
        params: ThreadStartParams,
    ) -> anyhow::Result<i64> {
        let params = Some(serde_json::to_value(params)?);
        self.send_request("thread/start", params).await
    }

    /// Send a `thread/resume` JSON-RPC request.
    pub async fn send_thread_resume_request(
        &mut self,
        params: ThreadResumeParams,
    ) -> anyhow::Result<i64> {
        let params = Some(serde_json::to_value(params)?);
        self.send_request("thread/resume", params).await
    }

    /// Send a `thread/fork` JSON-RPC request.
    pub async fn send_thread_fork_request(
        &mut self,
        params: ThreadForkParams,
    ) -> anyhow::Result<i64> {
        let params = Some(serde_json::to_value(params)?);
        self.send_request("thread/fork", params).await
    }

    /// Send a `thread/archive` JSON-RPC request.
    pub async fn send_thread_archive_request(
        &mut self,
        params: ThreadArchiveParams,
    ) -> anyhow::Result<i64> {
        let params = Some(serde_json::to_value(params)?);
        self.send_request("thread/archive", params).await
    }

    /// Send a `thread/name/set` JSON-RPC request.
    pub async fn send_thread_set_name_request(
        &mut self,
        params: ThreadSetNameParams,
    ) -> anyhow::Result<i64> {
        let params = Some(serde_json::to_value(params)?);
        self.send_request("thread/name/set", params).await
    }

    /// Send a `thread/metadata/update` JSON-RPC request.
    pub async fn send_thread_metadata_update_request(
        &mut self,
        params: ThreadMetadataUpdateParams,
    ) -> anyhow::Result<i64> {
        let params = Some(serde_json::to_value(params)?);
        self.send_request("thread/metadata/update", params).await
    }

    /// Send a `thread/unsubscribe` JSON-RPC request.
    pub async fn send_thread_unsubscribe_request(
        &mut self,
        params: ThreadUnsubscribeParams,
    ) -> anyhow::Result<i64> {
        let params = Some(serde_json::to_value(params)?);
        self.send_request("thread/unsubscribe", params).await
    }

    /// Send a `thread/unarchive` JSON-RPC request.
    pub async fn send_thread_unarchive_request(
        &mut self,
        params: ThreadUnarchiveParams,
    ) -> anyhow::Result<i64> {
        let params = Some(serde_json::to_value(params)?);
        self.send_request("thread/unarchive", params).await
    }

    /// Send a `thread/compact/start` JSON-RPC request.
    pub async fn send_thread_compact_start_request(
        &mut self,
        params: ThreadCompactStartParams,
    ) -> anyhow::Result<i64> {
        let params = Some(serde_json::to_value(params)?);
        self.send_request("thread/compact/start", params).await
    }

    /// Send a `thread/shellCommand` JSON-RPC request.
    pub async fn send_thread_shell_command_request(
        &mut self,
        params: ThreadShellCommandParams,
    ) -> anyhow::Result<i64> {
        let params = Some(serde_json::to_value(params)?);
        self.send_request("thread/shellCommand", params).await
    }

    /// Send a `thread/rollback` JSON-RPC request.
    pub async fn send_thread_rollback_request(
        &mut self,
        params: ThreadRollbackParams,
    ) -> anyhow::Result<i64> {
        let params = Some(serde_json::to_value(params)?);
        self.send_request("thread/rollback", params).await
    }

    /// Send a `thread/list` JSON-RPC request.
    pub async fn send_thread_list_request(
        &mut self,
        params: ThreadListParams,
    ) -> anyhow::Result<i64> {
        let params = Some(serde_json::to_value(params)?);
        self.send_request("thread/list", params).await
    }

    /// Send a `thread/loaded/list` JSON-RPC request.
    pub async fn send_thread_loaded_list_request(
        &mut self,
        params: ThreadLoadedListParams,
    ) -> anyhow::Result<i64> {
        let params = Some(serde_json::to_value(params)?);
        self.send_request("thread/loaded/list", params).await
    }

    /// Send a `thread/read` JSON-RPC request.
    pub async fn send_thread_read_request(
        &mut self,
        params: ThreadReadParams,
    ) -> anyhow::Result<i64> {
        let params = Some(serde_json::to_value(params)?);
        self.send_request("thread/read", params).await
    }

    /// Send a `thread/history/read` JSON-RPC request.
    pub async fn send_thread_history_read_request(
        &mut self,
        params: ThreadHistoryReadParams,
    ) -> anyhow::Result<i64> {
        let params = Some(serde_json::to_value(params)?);
        self.send_request("thread/history/read", params).await
    }

    /// Send a `model/list` JSON-RPC request.
    pub async fn send_list_models_request(
        &mut self,
        params: ModelListParams,
    ) -> anyhow::Result<i64> {
        let params = Some(serde_json::to_value(params)?);
        self.send_request("model/list", params).await
    }

    /// Send an `experimentalFeature/list` JSON-RPC request.
    pub async fn send_experimental_feature_list_request(
        &mut self,
        params: ExperimentalFeatureListParams,
    ) -> anyhow::Result<i64> {
        let params = Some(serde_json::to_value(params)?);
        self.send_request("experimentalFeature/list", params).await
    }

    /// Send an `experimentalFeature/enablement/set` JSON-RPC request.
    pub async fn send_experimental_feature_enablement_set_request(
        &mut self,
        params: praxis_app_gateway_protocol::ExperimentalFeatureEnablementSetParams,
    ) -> anyhow::Result<i64> {
        let params = Some(serde_json::to_value(params)?);
        self.send_request("experimentalFeature/enablement/set", params)
            .await
    }

    /// Send an `app/list` JSON-RPC request.
    pub async fn send_apps_list_request(&mut self, params: AppsListParams) -> anyhow::Result<i64> {
        let params = Some(serde_json::to_value(params)?);
        self.send_request("app/list", params).await
    }

    /// Send a `skills/list` JSON-RPC request.
    pub async fn send_skills_list_request(
        &mut self,
        params: SkillsListParams,
    ) -> anyhow::Result<i64> {
        let params = Some(serde_json::to_value(params)?);
        self.send_request("skills/list", params).await
    }

    /// Send a `plugin/install` JSON-RPC request.
    pub async fn send_plugin_install_request(
        &mut self,
        params: PluginInstallParams,
    ) -> anyhow::Result<i64> {
        let params = Some(serde_json::to_value(params)?);
        self.send_request("plugin/install", params).await
    }

    /// Send a `plugin/uninstall` JSON-RPC request.
    pub async fn send_plugin_uninstall_request(
        &mut self,
        params: PluginUninstallParams,
    ) -> anyhow::Result<i64> {
        let params = Some(serde_json::to_value(params)?);
        self.send_request("plugin/uninstall", params).await
    }

    /// Send a `plugin/catalog/list` JSON-RPC request.
    pub async fn send_plugin_list_request(
        &mut self,
        params: PluginListParams,
    ) -> anyhow::Result<i64> {
        let params = Some(serde_json::to_value(params)?);
        self.send_request("plugin/catalog/list", params).await
    }

    /// Send a `plugin/read` JSON-RPC request.
    pub async fn send_plugin_read_request(
        &mut self,
        params: PluginReadParams,
    ) -> anyhow::Result<i64> {
        let params = Some(serde_json::to_value(params)?);
        self.send_request("plugin/read", params).await
    }

    /// Send a JSON-RPC request with raw params for protocol-level validation tests.
    pub async fn send_raw_request(
        &mut self,
        method: &str,
        params: Option<serde_json::Value>,
    ) -> anyhow::Result<i64> {
        self.send_request(method, params).await
    }
    /// Send a `collaborationMode/list` JSON-RPC request.
    pub async fn send_list_collaboration_modes_request(
        &mut self,
        params: CollaborationModeListParams,
    ) -> anyhow::Result<i64> {
        let params = Some(serde_json::to_value(params)?);
        self.send_request("collaborationMode/list", params).await
    }

    /// Send a `mock/experimentalMethod` JSON-RPC request.
    pub async fn send_mock_experimental_method_request(
        &mut self,
        params: MockExperimentalMethodParams,
    ) -> anyhow::Result<i64> {
        let params = Some(serde_json::to_value(params)?);
        self.send_request("mock/experimentalMethod", params).await
    }

    /// Send a `turn/start` JSON-RPC request.
    pub async fn send_turn_start_request(
        &mut self,
        params: TurnStartParams,
    ) -> anyhow::Result<i64> {
        let params = Some(serde_json::to_value(params)?);
        self.send_request("turn/start", params).await
    }

    /// Send a `command/exec` JSON-RPC request.
    pub async fn send_command_exec_request(
        &mut self,
        params: CommandExecParams,
    ) -> anyhow::Result<i64> {
        let params = Some(serde_json::to_value(params)?);
        self.send_request("command/exec", params).await
    }

    /// Send a `command/exec/write` JSON-RPC request.
    pub async fn send_command_exec_write_request(
        &mut self,
        params: CommandExecWriteParams,
    ) -> anyhow::Result<i64> {
        let params = Some(serde_json::to_value(params)?);
        self.send_request("command/exec/write", params).await
    }

    /// Send a `command/exec/resize` JSON-RPC request.
    pub async fn send_command_exec_resize_request(
        &mut self,
        params: CommandExecResizeParams,
    ) -> anyhow::Result<i64> {
        let params = Some(serde_json::to_value(params)?);
        self.send_request("command/exec/resize", params).await
    }

    /// Send a `command/exec/terminate` JSON-RPC request.
    pub async fn send_command_exec_terminate_request(
        &mut self,
        params: CommandExecTerminateParams,
    ) -> anyhow::Result<i64> {
        let params = Some(serde_json::to_value(params)?);
        self.send_request("command/exec/terminate", params).await
    }

    /// Send a `turn/interrupt` JSON-RPC request.
    pub async fn send_turn_interrupt_request(
        &mut self,
        params: TurnInterruptParams,
    ) -> anyhow::Result<i64> {
        let params = Some(serde_json::to_value(params)?);
        self.send_request("turn/interrupt", params).await
    }

    /// Send a `thread/realtime/start` JSON-RPC request.
    pub async fn send_thread_realtime_start_request(
        &mut self,
        params: ThreadRealtimeStartParams,
    ) -> anyhow::Result<i64> {
        let params = Some(serde_json::to_value(params)?);
        self.send_request("thread/realtime/start", params).await
    }

    /// Send a `thread/realtime/appendAudio` JSON-RPC request.
    pub async fn send_thread_realtime_append_audio_request(
        &mut self,
        params: ThreadRealtimeAppendAudioParams,
    ) -> anyhow::Result<i64> {
        let params = Some(serde_json::to_value(params)?);
        self.send_request("thread/realtime/appendAudio", params)
            .await
    }

    /// Send a `thread/realtime/appendText` JSON-RPC request.
    pub async fn send_thread_realtime_append_text_request(
        &mut self,
        params: ThreadRealtimeAppendTextParams,
    ) -> anyhow::Result<i64> {
        let params = Some(serde_json::to_value(params)?);
        self.send_request("thread/realtime/appendText", params)
            .await
    }

    /// Send a `thread/realtime/stop` JSON-RPC request.
    pub async fn send_thread_realtime_stop_request(
        &mut self,
        params: ThreadRealtimeStopParams,
    ) -> anyhow::Result<i64> {
        let params = Some(serde_json::to_value(params)?);
        self.send_request("thread/realtime/stop", params).await
    }

    /// Deterministically clean up an intentionally in-flight turn.
    ///
    /// Some tests assert behavior while a turn is still running. Returning from those tests
    /// without an explicit interrupt + terminal turn notification wait can leave in-flight work
    /// racing teardown and intermittently show up as `LEAK` in nextest.
    ///
    /// In rare races, the turn can also fail or complete on its own after we send
    /// `turn/interrupt` but before the server emits the interrupt response. The helper treats a
    /// buffered matching `turn/completed` notification as sufficient terminal cleanup in that
    /// case so teardown does not flap on timing.
    pub async fn interrupt_turn_and_wait_for_aborted(
        &mut self,
        thread_id: String,
        turn_id: String,
        read_timeout: std::time::Duration,
    ) -> anyhow::Result<()> {
        let interrupt_request_id = self
            .send_turn_interrupt_request(TurnInterruptParams {
                thread_id: thread_id.clone(),
                turn_id: turn_id.clone(),
            })
            .await?;
        match tokio::time::timeout(
            read_timeout,
            self.read_stream_until_response_message(RequestId::Integer(interrupt_request_id)),
        )
        .await
        {
            Ok(result) => {
                result.with_context(|| "failed while waiting for turn interrupt response")?;
            }
            Err(err) => {
                if self.pending_turn_completed_notification(&thread_id, &turn_id) {
                    return Ok(());
                }
                return Err(err).with_context(|| "timed out waiting for turn interrupt response");
            }
        }
        match tokio::time::timeout(
            read_timeout,
            self.read_stream_until_notification_message("turn/completed"),
        )
        .await
        {
            Ok(result) => {
                result.with_context(|| "failed while waiting for terminal turn notification")?;
            }
            Err(err) => {
                if self.pending_turn_completed_notification(&thread_id, &turn_id) {
                    return Ok(());
                }
                return Err(err)
                    .with_context(|| "timed out waiting for terminal turn notification");
            }
        }
        Ok(())
    }

    /// Send a `turn/steer` JSON-RPC request.
    pub async fn send_turn_steer_request(
        &mut self,
        params: TurnSteerParams,
    ) -> anyhow::Result<i64> {
        let params = Some(serde_json::to_value(params)?);
        self.send_request("turn/steer", params).await
    }

    /// Send a `review/start` JSON-RPC request.
    pub async fn send_review_start_request(
        &mut self,
        params: ReviewStartParams,
    ) -> anyhow::Result<i64> {
        let params = Some(serde_json::to_value(params)?);
        self.send_request("review/start", params).await
    }

    pub async fn send_windows_sandbox_setup_start_request(
        &mut self,
        params: WindowsSandboxSetupStartParams,
    ) -> anyhow::Result<i64> {
        let params = Some(serde_json::to_value(params)?);
        self.send_request("windowsSandbox/setupStart", params).await
    }

    pub async fn send_config_read_request(
        &mut self,
        params: ConfigReadParams,
    ) -> anyhow::Result<i64> {
        let params = Some(serde_json::to_value(params)?);
        self.send_request("config/read", params).await
    }

    pub async fn send_config_value_write_request(
        &mut self,
        params: ConfigValueWriteParams,
    ) -> anyhow::Result<i64> {
        let params = Some(serde_json::to_value(params)?);
        self.send_request("config/value/write", params).await
    }

    pub async fn send_config_batch_write_request(
        &mut self,
        params: ConfigBatchWriteParams,
    ) -> anyhow::Result<i64> {
        let params = Some(serde_json::to_value(params)?);
        self.send_request("config/batchWrite", params).await
    }

    pub async fn send_fs_read_file_request(
        &mut self,
        params: FsReadFileParams,
    ) -> anyhow::Result<i64> {
        let params = Some(serde_json::to_value(params)?);
        self.send_request("fs/readFile", params).await
    }

    pub async fn send_fs_write_file_request(
        &mut self,
        params: FsWriteFileParams,
    ) -> anyhow::Result<i64> {
        let params = Some(serde_json::to_value(params)?);
        self.send_request("fs/writeFile", params).await
    }

    pub async fn send_fs_create_directory_request(
        &mut self,
        params: FsCreateDirectoryParams,
    ) -> anyhow::Result<i64> {
        let params = Some(serde_json::to_value(params)?);
        self.send_request("fs/createDirectory", params).await
    }

    pub async fn send_fs_get_metadata_request(
        &mut self,
        params: FsGetMetadataParams,
    ) -> anyhow::Result<i64> {
        let params = Some(serde_json::to_value(params)?);
        self.send_request("fs/getMetadata", params).await
    }

    pub async fn send_fs_read_directory_request(
        &mut self,
        params: FsReadDirectoryParams,
    ) -> anyhow::Result<i64> {
        let params = Some(serde_json::to_value(params)?);
        self.send_request("fs/readDirectory", params).await
    }

    pub async fn send_fs_remove_request(&mut self, params: FsRemoveParams) -> anyhow::Result<i64> {
        let params = Some(serde_json::to_value(params)?);
        self.send_request("fs/remove", params).await
    }

    pub async fn send_fs_copy_request(&mut self, params: FsCopyParams) -> anyhow::Result<i64> {
        let params = Some(serde_json::to_value(params)?);
        self.send_request("fs/copy", params).await
    }

    pub async fn send_fs_watch_request(&mut self, params: FsWatchParams) -> anyhow::Result<i64> {
        let params = Some(serde_json::to_value(params)?);
        self.send_request("fs/watch", params).await
    }

    pub async fn send_fs_unwatch_request(
        &mut self,
        params: FsUnwatchParams,
    ) -> anyhow::Result<i64> {
        let params = Some(serde_json::to_value(params)?);
        self.send_request("fs/unwatch", params).await
    }

    /// Send an `account/logout` JSON-RPC request.
    pub async fn send_logout_account_request(&mut self) -> anyhow::Result<i64> {
        self.send_request("account/logout", /*params*/ None).await
    }

    /// Send an `account/login/start` JSON-RPC request for API key login.
    pub async fn send_login_account_api_key_request(
        &mut self,
        api_key: &str,
    ) -> anyhow::Result<i64> {
        let params = serde_json::json!({
            "type": "apiKey",
            "apiKey": api_key,
        });
        self.send_request("account/login/start", Some(params)).await
    }

    /// Send an `account/login/start` JSON-RPC request for ChatGPT login.
    pub async fn send_login_account_chatgpt_request(&mut self) -> anyhow::Result<i64> {
        let params = serde_json::json!({
            "type": "chatgpt"
        });
        self.send_request("account/login/start", Some(params)).await
    }

    /// Send an `account/login/start` JSON-RPC request for ChatGPT device code login.
    pub async fn send_login_account_chatgpt_device_code_request(&mut self) -> anyhow::Result<i64> {
        let params = serde_json::json!({
            "type": "chatgptDeviceCode"
        });
        self.send_request("account/login/start", Some(params)).await
    }

    /// Send an `account/login/cancel` JSON-RPC request.
    pub async fn send_cancel_login_account_request(
        &mut self,
        params: CancelLoginAccountParams,
    ) -> anyhow::Result<i64> {
        let params = Some(serde_json::to_value(params)?);
        self.send_request("account/login/cancel", params).await
    }

    /// Send a `fuzzyFileSearch` JSON-RPC request.
    pub async fn send_fuzzy_file_search_request(
        &mut self,
        query: &str,
        roots: Vec<String>,
        cancellation_token: Option<String>,
    ) -> anyhow::Result<i64> {
        let mut params = serde_json::json!({
            "query": query,
            "roots": roots,
        });
        if let Some(token) = cancellation_token {
            params["cancellationToken"] = serde_json::json!(token);
        }
        self.send_request("fuzzyFileSearch", Some(params)).await
    }

    pub async fn send_fuzzy_file_search_session_start_request(
        &mut self,
        session_id: &str,
        roots: Vec<String>,
    ) -> anyhow::Result<i64> {
        let params = serde_json::json!({
            "sessionId": session_id,
            "roots": roots,
        });
        self.send_request("fuzzyFileSearch/sessionStart", Some(params))
            .await
    }

    pub async fn start_fuzzy_file_search_session(
        &mut self,
        session_id: &str,
        roots: Vec<String>,
    ) -> anyhow::Result<JSONRPCResponse> {
        let request_id = self
            .send_fuzzy_file_search_session_start_request(session_id, roots)
            .await?;
        self.read_stream_until_response_message(RequestId::Integer(request_id))
            .await
    }

    pub async fn send_fuzzy_file_search_session_update_request(
        &mut self,
        session_id: &str,
        query: &str,
    ) -> anyhow::Result<i64> {
        let params = serde_json::json!({
            "sessionId": session_id,
            "query": query,
        });
        self.send_request("fuzzyFileSearch/sessionUpdate", Some(params))
            .await
    }

    pub async fn update_fuzzy_file_search_session(
        &mut self,
        session_id: &str,
        query: &str,
    ) -> anyhow::Result<JSONRPCResponse> {
        let request_id = self
            .send_fuzzy_file_search_session_update_request(session_id, query)
            .await?;
        self.read_stream_until_response_message(RequestId::Integer(request_id))
            .await
    }

    pub async fn send_fuzzy_file_search_session_stop_request(
        &mut self,
        session_id: &str,
    ) -> anyhow::Result<i64> {
        let params = serde_json::json!({
            "sessionId": session_id,
        });
        self.send_request("fuzzyFileSearch/sessionStop", Some(params))
            .await
    }

    pub async fn stop_fuzzy_file_search_session(
        &mut self,
        session_id: &str,
    ) -> anyhow::Result<JSONRPCResponse> {
        let request_id = self
            .send_fuzzy_file_search_session_stop_request(session_id)
            .await?;
        self.read_stream_until_response_message(RequestId::Integer(request_id))
            .await
    }
}
