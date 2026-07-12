use super::*;
use crate::outgoing_message::ConnectionId;
use crate::outgoing_message::ConnectionRequestId;
use crate::outgoing_message::RequestContext;
use praxis_app_gateway_protocol::ClientRequest;
use tracing::warn;

impl PraxisMessageProcessor {
    pub async fn process_request(
        &mut self,
        connection_id: ConnectionId,
        request: ClientRequest,
        app_gateway_client_name: Option<String>,
        request_context: RequestContext,
    ) {
        let to_connection_request_id =
            |request_id| ConnectionRequestId::new(connection_id, request_id);

        match request {
            ClientRequest::Initialize { .. } => {
                panic!("Initialize should be handled in MessageProcessor");
            }
            ClientRequest::ThreadStart { request_id, params } => {
                self.thread_start(
                    to_connection_request_id(request_id),
                    params,
                    request_context,
                )
                .await;
            }
            ClientRequest::ThreadUnsubscribe { request_id, params } => {
                self.thread_unsubscribe(to_connection_request_id(request_id), params)
                    .await;
            }
            ClientRequest::ThreadResume { request_id, params } => {
                self.thread_resume(to_connection_request_id(request_id), params)
                    .await;
            }
            ClientRequest::ThreadFork { request_id, params } => {
                self.thread_fork(to_connection_request_id(request_id), params)
                    .await;
            }
            ClientRequest::ThreadArchive { request_id, params } => {
                self.thread_archive(to_connection_request_id(request_id), params)
                    .await;
            }
            ClientRequest::ThreadDelete { request_id, params } => {
                self.thread_delete(to_connection_request_id(request_id), params)
                    .await;
            }
            ClientRequest::ThreadIncrementElicitation { request_id, params } => {
                self.thread_increment_elicitation(to_connection_request_id(request_id), params)
                    .await;
            }
            ClientRequest::ThreadDecrementElicitation { request_id, params } => {
                self.thread_decrement_elicitation(to_connection_request_id(request_id), params)
                    .await;
            }
            ClientRequest::ThreadSetName { request_id, params } => {
                self.thread_set_name(to_connection_request_id(request_id), params)
                    .await;
            }
            ClientRequest::ThreadRegenerateName { request_id, params } => {
                self.thread_regenerate_name(to_connection_request_id(request_id), params)
                    .await;
            }
            ClientRequest::ThreadModelSet { request_id, params } => {
                self.thread_model_set(to_connection_request_id(request_id), params)
                    .await;
            }
            ClientRequest::ThreadMetadataUpdate { request_id, params } => {
                self.thread_metadata_update(to_connection_request_id(request_id), params)
                    .await;
            }
            ClientRequest::ThreadUnarchive { request_id, params } => {
                self.thread_unarchive(to_connection_request_id(request_id), params)
                    .await;
            }
            ClientRequest::ThreadCompactStart { request_id, params } => {
                self.thread_compact_start(to_connection_request_id(request_id), params)
                    .await;
            }
            ClientRequest::ThreadBackgroundTerminalsClean { request_id, params } => {
                self.thread_background_terminals_clean(
                    to_connection_request_id(request_id),
                    params,
                )
                .await;
            }
            ClientRequest::ThreadRollback { request_id, params } => {
                self.thread_rollback(to_connection_request_id(request_id), params)
                    .await;
            }
            ClientRequest::ThreadList { request_id, params } => {
                self.thread_list(to_connection_request_id(request_id), params)
                    .await;
            }
            ClientRequest::ThreadLookup { request_id, params } => {
                self.thread_lookup(to_connection_request_id(request_id), params)
                    .await;
            }
            ClientRequest::ThreadLoadedList { request_id, params } => {
                self.thread_loaded_list(to_connection_request_id(request_id), params)
                    .await;
            }
            ClientRequest::ThreadRead { request_id, params } => {
                self.thread_read(to_connection_request_id(request_id), params)
                    .await;
            }
            ClientRequest::ThreadHistoryRead { request_id, params } => {
                self.thread_history_read(to_connection_request_id(request_id), params)
                    .await;
            }
            ClientRequest::ThreadGoalGet { request_id, params } => {
                self.thread_goal_get(to_connection_request_id(request_id), params)
                    .await;
            }
            ClientRequest::ThreadGoalSet { request_id, params } => {
                self.thread_goal_set(to_connection_request_id(request_id), params)
                    .await;
            }
            ClientRequest::ThreadGoalUpdate { request_id, params } => {
                self.thread_goal_update(to_connection_request_id(request_id), params)
                    .await;
            }
            ClientRequest::ThreadGoalClear { request_id, params } => {
                self.thread_goal_clear(to_connection_request_id(request_id), params)
                    .await;
            }
            ClientRequest::ThreadHeartbeatGet { request_id, params } => {
                self.thread_heartbeat_get(to_connection_request_id(request_id), params)
                    .await;
            }
            ClientRequest::ThreadHeartbeatSet { request_id, params } => {
                self.thread_heartbeat_set(to_connection_request_id(request_id), params)
                    .await;
            }
            ClientRequest::ThreadHeartbeatClear { request_id, params } => {
                self.thread_heartbeat_clear(to_connection_request_id(request_id), params)
                    .await;
            }
            ClientRequest::WorkspaceChangeGet { request_id, params } => {
                self.workspace_change_get(to_connection_request_id(request_id), params)
                    .await;
            }
            ClientRequest::WorkspaceChangeReviewHunk { request_id, params } => {
                self.workspace_change_review_hunk(to_connection_request_id(request_id), params)
                    .await;
            }
            ClientRequest::AutomationList { request_id, params } => {
                self.automation_list(to_connection_request_id(request_id), params)
                    .await;
            }
            ClientRequest::AutomationGet { request_id, params } => {
                self.automation_get(to_connection_request_id(request_id), params)
                    .await;
            }
            ClientRequest::AutomationCreate { request_id, params } => {
                self.automation_create(to_connection_request_id(request_id), params)
                    .await;
            }
            ClientRequest::AutomationUpdate { request_id, params } => {
                self.automation_update(to_connection_request_id(request_id), params)
                    .await;
            }
            ClientRequest::AutomationDelete { request_id, params } => {
                self.automation_delete(to_connection_request_id(request_id), params)
                    .await;
            }
            ClientRequest::AutomationHistory { request_id, params } => {
                self.automation_history(to_connection_request_id(request_id), params)
                    .await;
            }
            ClientRequest::AutomationRunNow { request_id, params } => {
                self.automation_run_now(to_connection_request_id(request_id), params)
                    .await;
            }
            ClientRequest::ThreadControlSnapshot { request_id, params } => {
                self.thread_control_snapshot(to_connection_request_id(request_id), params)
                    .await;
            }
            ClientRequest::ThreadControlClaim { request_id, params } => {
                self.thread_control_claim(to_connection_request_id(request_id), params)
                    .await;
            }
            ClientRequest::ThreadControlRelease { request_id, params } => {
                self.thread_control_release(to_connection_request_id(request_id), params)
                    .await;
            }
            ClientRequest::ThreadControlQueue { request_id, params } => {
                self.thread_control_queue(to_connection_request_id(request_id), params)
                    .await;
            }
            ClientRequest::ThreadControlQueueCancel { request_id, params } => {
                self.thread_control_queue_cancel(to_connection_request_id(request_id), params)
                    .await;
            }
            ClientRequest::ThreadControlQueueFlush { request_id, params } => {
                self.thread_control_queue_flush(to_connection_request_id(request_id), params)
                    .await;
            }
            ClientRequest::ThreadShellCommand { request_id, params } => {
                self.thread_shell_command(to_connection_request_id(request_id), params)
                    .await;
            }
            ClientRequest::ThreadHistoryAppend { request_id, params } => {
                self.thread_history_append(to_connection_request_id(request_id), params)
                    .await;
            }
            ClientRequest::ThreadHistoryEntryGet { request_id, params } => {
                self.thread_history_entry_get(to_connection_request_id(request_id), params)
                    .await;
            }
            ClientRequest::SkillsList { request_id, params } => {
                self.skills_list(to_connection_request_id(request_id), params)
                    .await;
            }
            ClientRequest::PluginList { request_id, params } => {
                self.plugin_list(to_connection_request_id(request_id), params)
                    .await;
            }
            ClientRequest::PluginRead { request_id, params } => {
                self.plugin_read(to_connection_request_id(request_id), params)
                    .await;
            }
            ClientRequest::PluginSync { request_id, params } => {
                self.plugin_sync(to_connection_request_id(request_id), params)
                    .await;
            }
            ClientRequest::PluginCommandExecute { request_id, params } => {
                self.plugin_command_execute(to_connection_request_id(request_id), params)
                    .await;
            }
            ClientRequest::AppsList { request_id, params } => {
                self.apps_list(to_connection_request_id(request_id), params)
                    .await;
            }
            ClientRequest::SkillsConfigWrite { request_id, params } => {
                self.skills_config_write(to_connection_request_id(request_id), params)
                    .await;
            }
            ClientRequest::PluginInstall { request_id, params } => {
                self.plugin_install(to_connection_request_id(request_id), params)
                    .await;
            }
            ClientRequest::PluginUninstall { request_id, params } => {
                self.plugin_uninstall(to_connection_request_id(request_id), params)
                    .await;
            }
            ClientRequest::PluginSetEnabled { request_id, params } => {
                self.plugin_set_enabled(to_connection_request_id(request_id), params)
                    .await;
            }
            ClientRequest::TurnStart { request_id, params } => {
                self.turn_start(
                    to_connection_request_id(request_id),
                    params,
                    app_gateway_client_name.clone(),
                )
                .await;
            }
            ClientRequest::TurnSteer { request_id, params } => {
                self.turn_steer(to_connection_request_id(request_id), params)
                    .await;
            }
            ClientRequest::TurnInterrupt { request_id, params } => {
                self.turn_interrupt(to_connection_request_id(request_id), params)
                    .await;
            }
            ClientRequest::ThreadRealtimeStart { request_id, params } => {
                self.thread_realtime_start(to_connection_request_id(request_id), params)
                    .await;
            }
            ClientRequest::ThreadRealtimeAppendAudio { request_id, params } => {
                self.thread_realtime_append_audio(to_connection_request_id(request_id), params)
                    .await;
            }
            ClientRequest::AudioTranscribe { request_id, params } => {
                self.audio_transcribe(to_connection_request_id(request_id), params)
                    .await;
            }
            ClientRequest::ThreadRealtimeAppendText { request_id, params } => {
                self.thread_realtime_append_text(to_connection_request_id(request_id), params)
                    .await;
            }
            ClientRequest::ThreadRealtimeStop { request_id, params } => {
                self.thread_realtime_stop(to_connection_request_id(request_id), params)
                    .await;
            }
            ClientRequest::ReviewStart { request_id, params } => {
                self.review_start(to_connection_request_id(request_id), params)
                    .await;
            }
            ClientRequest::ModelList { request_id, params } => {
                self.model_list(to_connection_request_id(request_id), params)
                    .await;
            }
            ClientRequest::ExperimentalFeatureList { request_id, params } => {
                self.experimental_feature_list(to_connection_request_id(request_id), params)
                    .await;
            }
            ClientRequest::CollaborationModeList { request_id, params } => {
                self.collaboration_mode_list(to_connection_request_id(request_id), params)
                    .await;
            }
            ClientRequest::MockExperimentalMethod { request_id, params } => {
                self.mock_experimental_method(to_connection_request_id(request_id), params)
                    .await;
            }
            ClientRequest::McpServerOauthLogin { request_id, params } => {
                self.mcp_server_oauth_login(to_connection_request_id(request_id), params)
                    .await;
            }
            ClientRequest::McpServerRefresh { request_id, params } => {
                self.mcp_server_refresh(to_connection_request_id(request_id), params)
                    .await;
            }
            ClientRequest::McpServerStatusList { request_id, params } => {
                self.list_mcp_server_status(to_connection_request_id(request_id), params)
                    .await;
            }
            ClientRequest::WindowsSandboxSetupStart { request_id, params } => {
                self.windows_sandbox_setup_start(to_connection_request_id(request_id), params)
                    .await;
            }
            ClientRequest::LoginAccount { request_id, params } => {
                self.login_account(to_connection_request_id(request_id), params)
                    .await;
            }
            ClientRequest::LogoutAccount {
                request_id,
                params: _,
            } => {
                self.logout_account(to_connection_request_id(request_id))
                    .await;
            }
            ClientRequest::CancelLoginAccount { request_id, params } => {
                self.cancel_login_account(to_connection_request_id(request_id), params)
                    .await;
            }
            ClientRequest::GetAccount { request_id, params } => {
                self.get_account(to_connection_request_id(request_id), params)
                    .await;
            }
            ClientRequest::FuzzyFileSearch { request_id, params } => {
                self.fuzzy_file_search(to_connection_request_id(request_id), params)
                    .await;
            }
            ClientRequest::FuzzyFileSearchSessionStart { request_id, params } => {
                self.fuzzy_file_search_session_start(to_connection_request_id(request_id), params)
                    .await;
            }
            ClientRequest::FuzzyFileSearchSessionUpdate { request_id, params } => {
                self.fuzzy_file_search_session_update(to_connection_request_id(request_id), params)
                    .await;
            }
            ClientRequest::FuzzyFileSearchSessionStop { request_id, params } => {
                self.fuzzy_file_search_session_stop(to_connection_request_id(request_id), params)
                    .await;
            }
            ClientRequest::OneOffCommandExec { request_id, params } => {
                self.exec_one_off_command(to_connection_request_id(request_id), params)
                    .await;
            }
            ClientRequest::CommandExecWrite { request_id, params } => {
                self.command_exec_write(to_connection_request_id(request_id), params)
                    .await;
            }
            ClientRequest::CommandExecResize { request_id, params } => {
                self.command_exec_resize(to_connection_request_id(request_id), params)
                    .await;
            }
            ClientRequest::CommandExecTerminate { request_id, params } => {
                self.command_exec_terminate(to_connection_request_id(request_id), params)
                    .await;
            }
            ClientRequest::ConfigRead { .. }
            | ClientRequest::ConfigValueWrite { .. }
            | ClientRequest::ConfigBatchWrite { .. }
            | ClientRequest::ExperimentalFeatureEnablementSet { .. } => {
                Self::warn_unexpected_forwarded_request("Config request");
            }
            ClientRequest::FsReadFile { .. }
            | ClientRequest::FsWriteFile { .. }
            | ClientRequest::FsCreateDirectory { .. }
            | ClientRequest::FsGetMetadata { .. }
            | ClientRequest::FsReadDirectory { .. }
            | ClientRequest::FsRemove { .. }
            | ClientRequest::FsCopy { .. }
            | ClientRequest::FsWatch { .. }
            | ClientRequest::FsUnwatch { .. } => {
                Self::warn_unexpected_forwarded_request("Filesystem request");
            }
            ClientRequest::ConfigRequirementsRead { .. } => {
                Self::warn_unexpected_forwarded_request("ConfigRequirementsRead request");
            }
            ClientRequest::ExternalAgentConfigDetect { .. }
            | ClientRequest::ExternalAgentConfigImport { .. } => {
                Self::warn_unexpected_forwarded_request("ExternalAgentConfig request");
            }
            ClientRequest::GetAccountRateLimits {
                request_id,
                params: _,
            } => {
                self.get_account_rate_limits(to_connection_request_id(request_id))
                    .await;
            }
            ClientRequest::FeedbackUpload { request_id, params } => {
                self.upload_feedback(to_connection_request_id(request_id), params)
                    .await;
            }
        }
    }

    fn warn_unexpected_forwarded_request(category: &str) {
        warn!("{category} reached PraxisMessageProcessor unexpectedly");
    }
}
