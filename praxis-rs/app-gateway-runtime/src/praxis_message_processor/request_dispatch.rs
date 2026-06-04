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
        let to_connection_request_id = |request_id| ConnectionRequestId {
            connection_id,
            request_id,
        };

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
            ClientRequest::ThreadLoadedList { request_id, params } => {
                self.thread_loaded_list(to_connection_request_id(request_id), params)
                    .await;
            }
            ClientRequest::ThreadRead { request_id, params } => {
                self.thread_read(to_connection_request_id(request_id), params)
                    .await;
            }
            ClientRequest::ThreadControlAcquire { request_id, params } => {
                self.thread_control_acquire(to_connection_request_id(request_id), params)
                    .await;
            }
            ClientRequest::ThreadControlRelease { request_id, params } => {
                self.thread_control_release(to_connection_request_id(request_id), params)
                    .await;
            }
            ClientRequest::ThreadShellCommand { request_id, params } => {
                self.thread_shell_command(to_connection_request_id(request_id), params)
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
                warn!("Config request reached PraxisMessageProcessor unexpectedly");
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
                warn!("Filesystem request reached PraxisMessageProcessor unexpectedly");
            }
            ClientRequest::ConfigRequirementsRead { .. } => {
                warn!("ConfigRequirementsRead request reached PraxisMessageProcessor unexpectedly");
            }
            ClientRequest::ExternalAgentConfigDetect { .. }
            | ClientRequest::ExternalAgentConfigImport { .. } => {
                warn!("ExternalAgentConfig request reached PraxisMessageProcessor unexpectedly");
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
}
