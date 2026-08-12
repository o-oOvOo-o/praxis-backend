use super::FuzzyFileSearchParams;
use super::FuzzyFileSearchResponse;
use super::FuzzyFileSearchSessionStartParams;
use super::FuzzyFileSearchSessionStartResponse;
use super::FuzzyFileSearchSessionStopParams;
use super::FuzzyFileSearchSessionStopResponse;
use super::FuzzyFileSearchSessionUpdateParams;
use super::FuzzyFileSearchSessionUpdateResponse;
use crate::RequestId;
use crate::export::GeneratedSchema;
use crate::export::write_json_schema;
use crate::protocol::api;
use schemars::JsonSchema;
use serde::Deserialize;
use serde::Serialize;
use ts_rs::TS;

macro_rules! experimental_reason_expr {
    // If a request variant is explicitly marked experimental, that reason wins.
    (variant $variant:ident, #[experimental($reason:expr)] $params:ident $(, $inspect_params:tt)?) => {
        Some($reason)
    };
    // `inspect_params: true` is used when a method is mostly stable but needs
    // field-level gating from its params type (for example, ThreadStart).
    (variant $variant:ident, $params:ident, true) => {
        crate::experimental_api::ExperimentalApi::experimental_reason($params)
    };
    (variant $variant:ident, $params:ident $(, $inspect_params:tt)?) => {
        None
    };
}

macro_rules! experimental_method_entry {
    (#[experimental($reason:expr)] => $wire:literal) => {
        $wire
    };
    (#[experimental($reason:expr)]) => {
        $reason
    };
    ($($tt:tt)*) => {
        ""
    };
}

macro_rules! experimental_type_entry {
    (#[experimental($reason:expr)] $ty:ty) => {
        stringify!($ty)
    };
    ($ty:ty) => {
        ""
    };
}

/// Generates an `enum ClientRequest` where each variant is a request that the
/// client can send to the server. Each variant has associated `params` and
/// `response` types. Also generates a `export_client_responses()` function to
/// export all response types to TypeScript.
macro_rules! client_request_definitions {
    (
        $(
            $(#[experimental($reason:expr)])?
            $(#[doc = $variant_doc:literal])*
            $variant:ident $(=> $wire:literal)? {
                params: $(#[$params_meta:meta])* $params:ty,
                $(inspect_params: $inspect_params:tt,)?
                response: $response:ty,
            }
        ),* $(,)?
    ) => {
        /// Request from the client to the server.
        #[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema, TS)]
        #[serde(tag = "method", rename_all = "camelCase")]
        pub enum ClientRequest {
            $(
                $(#[doc = $variant_doc])*
                $(#[serde(rename = $wire)] #[ts(rename = $wire)])?
                $variant {
                    #[serde(rename = "id")]
                    request_id: RequestId,
                    $(#[$params_meta])*
                    params: $params,
                },
            )*
        }

        impl ClientRequest {
            pub fn id(&self) -> &RequestId {
                match self {
                    $(Self::$variant { request_id, .. } => request_id,)*
                }
            }

            pub fn method(&self) -> String {
                serde_json::to_value(self)
                    .ok()
                    .and_then(|value| {
                        value
                            .get("method")
                            .and_then(serde_json::Value::as_str)
                            .map(str::to_owned)
                    })
                    .unwrap_or_else(|| "<unknown>".to_string())
            }
        }

        /// Typed response from the server to the client.
        #[derive(Serialize, Deserialize, Debug, Clone)]
        #[serde(tag = "method", rename_all = "camelCase")]
        pub enum ClientResponse {
            $(
                $(#[doc = $variant_doc])*
                $(#[serde(rename = $wire)])?
                $variant {
                    #[serde(rename = "id")]
                    request_id: RequestId,
                    response: $response,
                },
            )*
        }

        impl ClientResponse {
            pub fn id(&self) -> &RequestId {
                match self {
                    $(Self::$variant { request_id, .. } => request_id,)*
                }
            }

            pub fn method(&self) -> String {
                serde_json::to_value(self)
                    .ok()
                    .and_then(|value| {
                        value
                            .get("method")
                            .and_then(serde_json::Value::as_str)
                            .map(str::to_owned)
                    })
                    .unwrap_or_else(|| "<unknown>".to_string())
            }
        }

        impl crate::experimental_api::ExperimentalApi for ClientRequest {
            fn experimental_reason(&self) -> Option<&'static str> {
                match self {
                    $(
                        Self::$variant { params: _params, .. } => {
                            experimental_reason_expr!(
                                variant $variant,
                                $(#[experimental($reason)])?
                                _params
                                $(, $inspect_params)?
                            )
                        }
                    )*
                }
            }
        }

        pub(crate) const EXPERIMENTAL_CLIENT_METHODS: &[&str] = &[
            $(
                experimental_method_entry!($(#[experimental($reason)])? $(=> $wire)?),
            )*
        ];
        pub(crate) const EXPERIMENTAL_CLIENT_METHOD_PARAM_TYPES: &[&str] = &[
            $(
                experimental_type_entry!($(#[experimental($reason)])? $params),
            )*
        ];
        pub(crate) const EXPERIMENTAL_CLIENT_METHOD_RESPONSE_TYPES: &[&str] = &[
            $(
                experimental_type_entry!($(#[experimental($reason)])? $response),
            )*
        ];

        pub fn export_client_responses(
            out_dir: &::std::path::Path,
        ) -> ::std::result::Result<(), ::ts_rs::ExportError> {
            $(
                <$response as ::ts_rs::TS>::export_all_to(out_dir)?;
            )*
            Ok(())
        }

        pub(crate) fn visit_client_response_types(v: &mut impl ::ts_rs::TypeVisitor) {
            $(
                v.visit::<$response>();
            )*
        }

        #[allow(clippy::vec_init_then_push)]
        pub fn export_client_response_schemas(
            out_dir: &::std::path::Path,
        ) -> ::anyhow::Result<Vec<GeneratedSchema>> {
            let mut schemas = Vec::new();
            $(
                schemas.push(write_json_schema::<$response>(out_dir, stringify!($response))?);
            )*
            Ok(schemas)
        }

        #[allow(clippy::vec_init_then_push)]
        pub fn export_client_param_schemas(
            out_dir: &::std::path::Path,
        ) -> ::anyhow::Result<Vec<GeneratedSchema>> {
            let mut schemas = Vec::new();
            $(
                schemas.push(write_json_schema::<$params>(out_dir, stringify!($params))?);
            )*
            Ok(schemas)
        }
    };
}

client_request_definitions! {
    Initialize {
        params: api::InitializeParams,
        response: api::InitializeResponse,
    },

    /// App-gateway APIs
    // Thread lifecycle
    // Uses `inspect_params` because only some fields are experimental.
    ThreadStart => "thread/start" {
        params: api::ThreadStartParams,
        inspect_params: true,
        response: api::ThreadStartResponse,
    },
    ThreadChildStart => "thread/child/start" {
        params: api::ThreadChildStartParams,
        inspect_params: true,
        response: api::ThreadStartResponse,
    },
    ThreadResume => "thread/resume" {
        params: api::ThreadResumeParams,
        inspect_params: true,
        response: api::ThreadResumeResponse,
    },
    ThreadFork => "thread/fork" {
        params: api::ThreadForkParams,
        inspect_params: true,
        response: api::ThreadForkResponse,
    },
    ThreadArchive => "thread/archive" {
        params: api::ThreadArchiveParams,
        response: api::ThreadArchiveResponse,
    },
    ThreadShare => "thread/share" {
        params: api::ThreadShareParams,
        response: api::ThreadShareResponse,
    },
    ThreadDelete => "thread/delete" {
        params: api::ThreadDeleteParams,
        response: api::ThreadDeleteResponse,
    },
    ThreadUnsubscribe => "thread/unsubscribe" {
        params: api::ThreadUnsubscribeParams,
        response: api::ThreadUnsubscribeResponse,
    },
    #[experimental("thread/increment_elicitation")]
    /// Increment the thread-local out-of-band elicitation counter.
    ///
    /// This is used by external helpers to pause timeout accounting while a user
    /// approval or other elicitation is pending outside the app-gateway request flow.
    ThreadIncrementElicitation => "thread/increment_elicitation" {
        params: api::ThreadIncrementElicitationParams,
        response: api::ThreadIncrementElicitationResponse,
    },
    #[experimental("thread/decrement_elicitation")]
    /// Decrement the thread-local out-of-band elicitation counter.
    ///
    /// When the count reaches zero, timeout accounting resumes for the thread.
    ThreadDecrementElicitation => "thread/decrement_elicitation" {
        params: api::ThreadDecrementElicitationParams,
        response: api::ThreadDecrementElicitationResponse,
    },
    ThreadSetName => "thread/name/set" {
        params: api::ThreadSetNameParams,
        response: api::ThreadSetNameResponse,
    },
    ThreadRegenerateName => "thread/name/regenerate" {
        params: api::ThreadRegenerateNameParams,
        response: api::ThreadRegenerateNameResponse,
    },
    ThreadModelSet => "thread/model/set" {
        params: api::ThreadModelSetParams,
        response: api::ThreadModelSetResponse,
    },
    ThreadPermissionsSet => "thread/permissions/set" {
        params: api::ThreadPermissionsSetParams,
        response: api::ThreadPermissionsSetResponse,
    },
    ThreadMetadataUpdate => "thread/metadata/update" {
        params: api::ThreadMetadataUpdateParams,
        response: api::ThreadMetadataUpdateResponse,
    },
    ThreadUnarchive => "thread/unarchive" {
        params: api::ThreadUnarchiveParams,
        response: api::ThreadUnarchiveResponse,
    },
    ThreadCompactStart => "thread/compact/start" {
        params: api::ThreadCompactStartParams,
        response: api::ThreadCompactStartResponse,
    },
    ThreadShellCommand => "thread/shellCommand" {
        params: api::ThreadShellCommandParams,
        response: api::ThreadShellCommandResponse,
    },
    ThreadHistoryAppend => "thread/history/append" {
        params: api::ThreadHistoryAppendParams,
        response: api::ThreadHistoryAppendResponse,
    },
    ThreadHistoryEntryGet => "thread/history/get" {
        params: api::ThreadHistoryEntryGetParams,
        response: api::ThreadHistoryEntryGetResponse,
    },
    #[experimental("thread/backgroundTerminals/clean")]
    ThreadBackgroundTerminalsClean => "thread/backgroundTerminals/clean" {
        params: api::ThreadBackgroundTerminalsCleanParams,
        response: api::ThreadBackgroundTerminalsCleanResponse,
    },
    ThreadRollback => "thread/rollback" {
        params: api::ThreadRollbackParams,
        response: api::ThreadRollbackResponse,
    },
    ThreadRewindPreview => "thread/rewind/preview" {
        params: api::ThreadRewindPreviewParams,
        response: api::ThreadRewindPreviewResponse,
    },
    ThreadList => "thread/list" {
        params: api::ThreadListParams,
        response: api::ThreadListResponse,
    },
    ExternalAgentSessionList => "externalAgentSession/list" {
        params: api::ExternalAgentSessionListParams,
        response: api::ThreadListResponse,
    },
    ThreadLookup => "thread/lookup" {
        params: api::ThreadLookupParams,
        response: api::ThreadLookupResponse,
    },
    ThreadLoadedList => "thread/loaded/list" {
        params: api::ThreadLoadedListParams,
        response: api::ThreadLoadedListResponse,
    },
    ThreadRead => "thread/read" {
        params: api::ThreadReadParams,
        response: api::ThreadReadResponse,
    },
    ThreadHistoryRead => "thread/history/read" {
        params: api::ThreadHistoryReadParams,
        response: api::ThreadHistoryReadResponse,
    },
    ThreadGoalGet => "thread/goal/get" {
        params: api::ThreadGoalGetParams,
        response: api::ThreadGoalGetResponse,
    },
    ThreadGoalSet => "thread/goal/set" {
        params: api::ThreadGoalSetParams,
        response: api::ThreadGoalSetResponse,
    },
    ThreadGoalUpdate => "thread/goal/update" {
        params: api::ThreadGoalUpdateParams,
        response: api::ThreadGoalUpdateResponse,
    },
    ThreadGoalClear => "thread/goal/clear" {
        params: api::ThreadGoalClearParams,
        response: api::ThreadGoalClearResponse,
    },
    ThreadHeartbeatGet => "thread/heartbeat/get" {
        params: api::ThreadHeartbeatGetParams,
        response: api::ThreadHeartbeatGetResponse,
    },
    ThreadHeartbeatSet => "thread/heartbeat/set" {
        params: api::ThreadHeartbeatSetParams,
        response: api::ThreadHeartbeatSetResponse,
    },
    ThreadHeartbeatClear => "thread/heartbeat/clear" {
        params: api::ThreadHeartbeatClearParams,
        response: api::ThreadHeartbeatClearResponse,
    },
    ThreadSelfworkGet => "thread/selfwork/get" {
        params: api::ThreadSelfworkGetParams,
        response: api::ThreadSelfworkGetResponse,
    },
    ThreadSelfworkStart => "thread/selfwork/start" {
        params: api::ThreadSelfworkStartParams,
        response: api::ThreadSelfworkStartResponse,
    },
    ThreadSelfworkStop => "thread/selfwork/stop" {
        params: api::ThreadSelfworkStopParams,
        response: api::ThreadSelfworkStopResponse,
    },
    WorkspaceChangeGet => "workspace/change/get" {
        params: api::WorkspaceChangeGetParams,
        response: api::WorkspaceChangeGetResponse,
    },
    WorkspaceChangeReviewHunk => "workspace/change/reviewHunk" {
        params: api::WorkspaceChangeReviewHunkParams,
        response: api::WorkspaceChangeReviewHunkResponse,
    },
    AutomationList => "automation/list" {
        params: api::AutomationListParams,
        response: api::AutomationListResponse,
    },
    AutomationGet => "automation/get" {
        params: api::AutomationGetParams,
        response: api::AutomationGetResponse,
    },
    AutomationCreate => "automation/create" {
        params: api::AutomationCreateParams,
        response: api::AutomationCreateResponse,
    },
    AutomationUpdate => "automation/update" {
        params: api::AutomationUpdateParams,
        response: api::AutomationUpdateResponse,
    },
    AutomationDelete => "automation/delete" {
        params: api::AutomationDeleteParams,
        response: api::AutomationDeleteResponse,
    },
    AutomationHistory => "automation/history" {
        params: api::AutomationHistoryParams,
        response: api::AutomationHistoryResponse,
    },
    AutomationRunNow => "automation/runNow" {
        params: api::AutomationRunNowParams,
        response: api::AutomationRunNowResponse,
    },
    #[experimental("thread/control/snapshot")]
    ThreadControlSnapshot => "thread/control/snapshot" {
        params: api::ThreadControlSnapshotParams,
        response: api::ThreadControlSnapshotResponse,
    },
    #[experimental("thread/control/claim")]
    ThreadControlClaim => "thread/control/claim" {
        params: api::ThreadControlClaimParams,
        response: api::ThreadControlClaimResponse,
    },
    #[experimental("thread/control/release")]
    ThreadControlRelease => "thread/control/release" {
        params: api::ThreadControlReleaseParams,
        response: api::ThreadControlReleaseResponse,
    },
    #[experimental("thread/control/queue")]
    ThreadControlQueue => "thread/control/queue" {
        params: api::ThreadControlQueueParams,
        response: api::ThreadControlQueueResponse,
    },
    #[experimental("thread/control/queue/cancel")]
    ThreadControlQueueCancel => "thread/control/queue/cancel" {
        params: api::ThreadControlQueueCancelParams,
        response: api::ThreadControlQueueCancelResponse,
    },
    #[experimental("thread/control/queue/flush")]
    ThreadControlQueueFlush => "thread/control/queue/flush" {
        params: api::ThreadControlQueueFlushParams,
        response: api::ThreadControlQueueFlushResponse,
    },
    SkillsList => "skills/list" {
        params: api::SkillsListParams,
        response: api::SkillsListResponse,
    },
    PluginList => "plugin/catalog/list" {
        params: api::PluginListParams,
        response: api::PluginListResponse,
    },
    PluginRead => "plugin/read" {
        params: api::PluginReadParams,
        response: api::PluginReadResponse,
    },
    PluginSync => "plugin/sync" {
        params: api::PluginSyncParams,
        response: api::PluginSyncResponse,
    },
    PluginCommandExecute => "pluginCommand/execute" {
        params: api::PluginCommandExecuteParams,
        response: api::PluginCommandExecuteResponse,
    },
    AppsList => "app/list" {
        params: api::AppsListParams,
        response: api::AppsListResponse,
    },
    FsReadFile => "fs/readFile" {
        params: api::FsReadFileParams,
        response: api::FsReadFileResponse,
    },
    FsWriteFile => "fs/writeFile" {
        params: api::FsWriteFileParams,
        response: api::FsWriteFileResponse,
    },
    FsCreateDirectory => "fs/createDirectory" {
        params: api::FsCreateDirectoryParams,
        response: api::FsCreateDirectoryResponse,
    },
    FsGetMetadata => "fs/getMetadata" {
        params: api::FsGetMetadataParams,
        response: api::FsGetMetadataResponse,
    },
    FsReadDirectory => "fs/readDirectory" {
        params: api::FsReadDirectoryParams,
        response: api::FsReadDirectoryResponse,
    },
    FsRemove => "fs/remove" {
        params: api::FsRemoveParams,
        response: api::FsRemoveResponse,
    },
    FsCopy => "fs/copy" {
        params: api::FsCopyParams,
        response: api::FsCopyResponse,
    },
    FsWatch => "fs/watch" {
        params: api::FsWatchParams,
        response: api::FsWatchResponse,
    },
    FsUnwatch => "fs/unwatch" {
        params: api::FsUnwatchParams,
        response: api::FsUnwatchResponse,
    },
    SkillsConfigWrite => "skills/config/write" {
        params: api::SkillsConfigWriteParams,
        response: api::SkillsConfigWriteResponse,
    },
    SkillsInstall => "skills/install" {
        params: api::SkillsInstallParams,
        response: api::SkillsInstallResponse,
    },
    SkillsUninstall => "skills/uninstall" {
        params: api::SkillsUninstallParams,
        response: api::SkillsUninstallResponse,
    },
    PluginInstall => "plugin/install" {
        params: api::PluginInstallParams,
        response: api::PluginInstallResponse,
    },
    PluginUninstall => "plugin/uninstall" {
        params: api::PluginUninstallParams,
        response: api::PluginUninstallResponse,
    },
    PluginSetEnabled => "plugin/setEnabled" {
        params: api::PluginSetEnabledParams,
        response: api::PluginSetEnabledResponse,
    },
    TurnStart => "turn/start" {
        params: api::TurnStartParams,
        inspect_params: true,
        response: api::TurnStartResponse,
    },
    TurnSteer => "turn/steer" {
        params: api::TurnSteerParams,
        response: api::TurnSteerResponse,
    },
    TurnInterrupt => "turn/interrupt" {
        params: api::TurnInterruptParams,
        response: api::TurnInterruptResponse,
    },
    #[experimental("thread/realtime/start")]
    ThreadRealtimeStart => "thread/realtime/start" {
        params: api::ThreadRealtimeStartParams,
        response: api::ThreadRealtimeStartResponse,
    },
    #[experimental("thread/realtime/appendAudio")]
    ThreadRealtimeAppendAudio => "thread/realtime/appendAudio" {
        params: api::ThreadRealtimeAppendAudioParams,
        response: api::ThreadRealtimeAppendAudioResponse,
    },
    #[experimental("audio/transcribe")]
    AudioTranscribe => "audio/transcribe" {
        params: api::AudioTranscribeParams,
        response: api::AudioTranscribeResponse,
    },
    #[experimental("thread/realtime/appendText")]
    ThreadRealtimeAppendText => "thread/realtime/appendText" {
        params: api::ThreadRealtimeAppendTextParams,
        response: api::ThreadRealtimeAppendTextResponse,
    },
    #[experimental("thread/realtime/stop")]
    ThreadRealtimeStop => "thread/realtime/stop" {
        params: api::ThreadRealtimeStopParams,
        response: api::ThreadRealtimeStopResponse,
    },
    ReviewStart => "review/start" {
        params: api::ReviewStartParams,
        response: api::ReviewStartResponse,
    },

    ModelList => "model/list" {
        params: api::ModelListParams,
        response: api::ModelListResponse,
    },
    ExperimentalFeatureList => "experimentalFeature/list" {
        params: api::ExperimentalFeatureListParams,
        response: api::ExperimentalFeatureListResponse,
    },
    ExperimentalFeatureEnablementSet => "experimentalFeature/enablement/set" {
        params: api::ExperimentalFeatureEnablementSetParams,
        response: api::ExperimentalFeatureEnablementSetResponse,
    },
    #[experimental("collaborationMode/list")]
    /// Lists collaboration mode presets.
    CollaborationModeList => "collaborationMode/list" {
        params: api::CollaborationModeListParams,
        response: api::CollaborationModeListResponse,
    },
    #[experimental("mock/experimentalMethod")]
    /// Test-only method used to validate experimental gating.
    MockExperimentalMethod => "mock/experimentalMethod" {
        params: api::MockExperimentalMethodParams,
        response: api::MockExperimentalMethodResponse,
    },

    McpServerOauthLogin => "mcpServer/oauth/login" {
        params: api::McpServerOauthLoginParams,
        response: api::McpServerOauthLoginResponse,
    },

    McpServerRefresh => "config/mcpServer/reload" {
        params: #[ts(type = "undefined")] #[serde(skip_serializing_if = "Option::is_none")] Option<()>,
        response: api::McpServerRefreshResponse,
    },

    McpServerStatusList => "mcpServerStatus/list" {
        params: api::ListMcpServerStatusParams,
        response: api::ListMcpServerStatusResponse,
    },

    WindowsSandboxSetupStart => "windowsSandbox/setupStart" {
        params: api::WindowsSandboxSetupStartParams,
        response: api::WindowsSandboxSetupStartResponse,
    },

    LoginAccount => "account/login/start" {
        params: api::LoginAccountParams,
        inspect_params: true,
        response: api::LoginAccountResponse,
    },

    CancelLoginAccount => "account/login/cancel" {
        params: api::CancelLoginAccountParams,
        response: api::CancelLoginAccountResponse,
    },

    LogoutAccount => "account/logout" {
        params: #[ts(type = "undefined")] #[serde(skip_serializing_if = "Option::is_none")] Option<()>,
        response: api::LogoutAccountResponse,
    },

    GetAccountRateLimits => "account/rateLimits/read" {
        params: #[ts(type = "undefined")] #[serde(skip_serializing_if = "Option::is_none")] Option<()>,
        response: api::GetAccountRateLimitsResponse,
    },

    FeedbackUpload => "feedback/upload" {
        params: api::FeedbackUploadParams,
        response: api::FeedbackUploadResponse,
    },

    /// Execute a standalone command (argv vector) under the server's sandbox.
    OneOffCommandExec => "command/exec" {
        params: api::CommandExecParams,
        response: api::CommandExecResponse,
    },
    /// Write stdin bytes to a running `command/exec` session or close stdin.
    CommandExecWrite => "command/exec/write" {
        params: api::CommandExecWriteParams,
        response: api::CommandExecWriteResponse,
    },
    /// Terminate a running `command/exec` session by client-supplied `processId`.
    CommandExecTerminate => "command/exec/terminate" {
        params: api::CommandExecTerminateParams,
        response: api::CommandExecTerminateResponse,
    },
    /// Resize a running PTY-backed `command/exec` session by client-supplied `processId`.
    CommandExecResize => "command/exec/resize" {
        params: api::CommandExecResizeParams,
        response: api::CommandExecResizeResponse,
    },

    ConfigRead => "config/read" {
        params: api::ConfigReadParams,
        response: api::ConfigReadResponse,
    },
    ExternalAgentConfigDetect => "externalAgentConfig/detect" {
        params: api::ExternalAgentConfigDetectParams,
        response: api::ExternalAgentConfigDetectResponse,
    },
    ExternalAgentConfigImport => "externalAgentConfig/import" {
        params: api::ExternalAgentConfigImportParams,
        response: api::ExternalAgentConfigImportResponse,
    },
    ConfigValueWrite => "config/value/write" {
        params: api::ConfigValueWriteParams,
        response: api::ConfigWriteResponse,
    },
    ConfigBatchWrite => "config/batchWrite" {
        params: api::ConfigBatchWriteParams,
        response: api::ConfigWriteResponse,
    },
    ModelProviderConfigWrite => "config/modelProvider/write" {
        params: api::ModelProviderConfigWriteParams,
        response: api::ModelProviderConfigWriteResponse,
    },
    ModelPreferencesWrite => "config/modelPreferences/write" {
        params: api::ModelPreferencesWriteParams,
        response: api::ModelPreferencesWriteResponse,
    },

    ConfigRequirementsRead => "configRequirements/read" {
        params: #[ts(type = "undefined")] #[serde(skip_serializing_if = "Option::is_none")] Option<()>,
        response: api::ConfigRequirementsReadResponse,
    },

    GetAccount => "account/read" {
        params: api::GetAccountParams,
        response: api::GetAccountResponse,
    },
    FuzzyFileSearch {
        params: FuzzyFileSearchParams,
        response: FuzzyFileSearchResponse,
    },
    #[experimental("fuzzyFileSearch/sessionStart")]
    FuzzyFileSearchSessionStart => "fuzzyFileSearch/sessionStart" {
        params: FuzzyFileSearchSessionStartParams,
        response: FuzzyFileSearchSessionStartResponse,
    },
    #[experimental("fuzzyFileSearch/sessionUpdate")]
    FuzzyFileSearchSessionUpdate => "fuzzyFileSearch/sessionUpdate" {
        params: FuzzyFileSearchSessionUpdateParams,
        response: FuzzyFileSearchSessionUpdateResponse,
    },
    #[experimental("fuzzyFileSearch/sessionStop")]
    FuzzyFileSearchSessionStop => "fuzzyFileSearch/sessionStop" {
        params: FuzzyFileSearchSessionStopParams,
        response: FuzzyFileSearchSessionStopResponse,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClientRequestLifecycleClass {
    Connect,
    Standard,
    TurnSubmission,
    LongRunning,
}

impl ClientRequest {
    pub const fn lifecycle_class(&self) -> ClientRequestLifecycleClass {
        match self {
            Self::Initialize { .. } => ClientRequestLifecycleClass::Connect,
            Self::TurnStart { .. } | Self::TurnSteer { .. } => {
                ClientRequestLifecycleClass::TurnSubmission
            }
            Self::ThreadStart { .. }
            | Self::ThreadChildStart { .. }
            | Self::ThreadResume { .. }
            | Self::ThreadFork { .. }
            | Self::ExternalAgentSessionList { .. }
            | Self::SkillsInstall { .. }
            | Self::PluginInstall { .. }
            | Self::AudioTranscribe { .. } => ClientRequestLifecycleClass::LongRunning,
            _ => ClientRequestLifecycleClass::Standard,
        }
    }
}
