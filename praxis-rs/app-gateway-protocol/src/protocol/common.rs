use std::collections::BTreeMap;
use std::path::Path;

use crate::JSONRPCNotification;
use crate::JSONRPCRequest;
use crate::RequestId;
use crate::export::GeneratedSchema;
use crate::export::write_json_schema;
use crate::protocol::api;
use praxis_experimental_api_macros::ExperimentalApi;
use schemars::JsonSchema;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value as JsonValue;
use strum_macros::Display;
use ts_rs::TS;

pub use praxis_protocol::auth::AuthMode;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct GatewayClientInfo {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<JsonValue>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub enum GatewayMode {
    Native,
    Service,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub enum GatewayTransport {
    Native,
    Stdio,
    WebSocket,
    NamedPipe,
    UnixSocket,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq, JsonSchema, TS)]
pub struct GatewayMetadata(pub BTreeMap<String, JsonValue>);

impl GatewayMetadata {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, Default, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub enum HostKind {
    Desktop,
    Editor,
    Cli,
    Service,
    #[default]
    Unknown,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub enum GatewayCapabilityKind {
    HostCommand,
    HostSurface,
    MetraCommand,
    MetraSurface,
    ProductBridge,
    SemanticTree,
    Input,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct GatewayCapability {
    pub kind: GatewayCapabilityKind,
    pub version: u32,
    #[serde(default, skip_serializing_if = "GatewayMetadata::is_empty")]
    pub metadata: GatewayMetadata,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct HostExtensionInfo {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default)]
    pub host_kind: HostKind,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub capabilities: Vec<GatewayCapability>,
    #[serde(default, skip_serializing_if = "GatewayMetadata::is_empty")]
    pub metadata: GatewayMetadata,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct MetraBridgeDescriptor {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub surfaces: Vec<MetraSurfaceDescriptor>,
    #[serde(default, skip_serializing_if = "GatewayMetadata::is_empty")]
    pub metadata: GatewayMetadata,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct MetraSurfaceDescriptor {
    pub id: String,
    pub title: String,
    pub surface_type: String,
    #[serde(default, skip_serializing_if = "GatewayMetadata::is_empty")]
    pub metadata: GatewayMetadata,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct MetraSemanticSnapshot {
    pub surface_id: String,
    pub revision: u64,
    pub tree: JsonValue,
    #[serde(default, skip_serializing_if = "GatewayMetadata::is_empty")]
    pub metadata: GatewayMetadata,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct GatewayRequestEnvelope {
    pub id: RequestId,
    pub method: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload: Option<JsonValue>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct GatewayResponseEnvelope {
    pub id: RequestId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload: Option<JsonValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<GatewayErrorPayload>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct GatewayEventEnvelope {
    pub method: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload: Option<JsonValue>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct GatewayErrorPayload {
    pub code: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<JsonValue>,
}

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
    ThreadList => "thread/list" {
        params: api::ThreadListParams,
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

/// Generates an `enum ServerRequest` where each variant is a request that the
/// server can send to the client along with the corresponding params and
/// response types. It also generates helper types used by the app-gateway
/// infrastructure (payload enum, request constructor, and export helpers).
macro_rules! server_request_definitions {
    (
        $(
            $(#[$variant_meta:meta])*
            $variant:ident $(=> $wire:literal)? {
                params: $params:ty,
                response: $response:ty,
            }
        ),* $(,)?
    ) => {
        /// Request initiated from the server and sent to the client.
        #[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema, TS)]
        #[allow(clippy::large_enum_variant)]
        #[serde(tag = "method", rename_all = "camelCase")]
        pub enum ServerRequest {
            $(
                $(#[$variant_meta])*
                $(#[serde(rename = $wire)] #[ts(rename = $wire)])?
                $variant {
                    #[serde(rename = "id")]
                    request_id: RequestId,
                    params: $params,
                },
            )*
        }

        impl ServerRequest {
            pub fn id(&self) -> &RequestId {
                match self {
                    $(Self::$variant { request_id, .. } => request_id,)*
                }
            }
        }

        #[derive(Debug, Clone, PartialEq, JsonSchema)]
        #[allow(clippy::large_enum_variant)]
        pub enum ServerRequestPayload {
            $( $variant($params), )*
        }

        impl ServerRequestPayload {
            pub fn request_with_id(self, request_id: RequestId) -> ServerRequest {
                match self {
                    $(Self::$variant(params) => ServerRequest::$variant { request_id, params },)*
                }
            }
        }

        pub fn export_server_responses(
            out_dir: &::std::path::Path,
        ) -> ::std::result::Result<(), ::ts_rs::ExportError> {
            $(
                <$response as ::ts_rs::TS>::export_all_to(out_dir)?;
            )*
            Ok(())
        }

        pub(crate) fn visit_server_response_types(v: &mut impl ::ts_rs::TypeVisitor) {
            $(
                v.visit::<$response>();
            )*
        }

        #[allow(clippy::vec_init_then_push)]
        pub fn export_server_response_schemas(
            out_dir: &Path,
        ) -> ::anyhow::Result<Vec<GeneratedSchema>> {
            let mut schemas = Vec::new();
            $(
                schemas.push(crate::export::write_json_schema::<$response>(
                    out_dir,
                    concat!(stringify!($variant), "Response"),
                )?);
            )*
            Ok(schemas)
        }

        #[allow(clippy::vec_init_then_push)]
        pub fn export_server_param_schemas(
            out_dir: &Path,
        ) -> ::anyhow::Result<Vec<GeneratedSchema>> {
            let mut schemas = Vec::new();
            $(
                schemas.push(crate::export::write_json_schema::<$params>(
                    out_dir,
                    concat!(stringify!($variant), "Params"),
                )?);
            )*
            Ok(schemas)
        }
    };
}

/// Generates `ServerNotification` enum and helpers, including a JSON Schema
/// exporter for each notification.
macro_rules! server_notification_definitions {
    (
        $(
            $(#[$variant_meta:meta])*
            $variant:ident $(=> $wire:literal)? ( $payload:ty )
        ),* $(,)?
    ) => {
        /// Notification sent from the server to the client.
        #[derive(
            Serialize,
            Deserialize,
            Debug,
            Clone,
            JsonSchema,
            TS,
            Display,
            ExperimentalApi,
        )]
        #[serde(tag = "method", content = "params", rename_all = "camelCase")]
        #[strum(serialize_all = "camelCase")]
        pub enum ServerNotification {
            $(
                $(#[$variant_meta])*
                $(#[serde(rename = $wire)] #[ts(rename = $wire)] #[strum(serialize = $wire)])?
                $variant($payload),
            )*
        }

        impl ServerNotification {
            pub fn to_params(self) -> Result<serde_json::Value, serde_json::Error> {
                match self {
                    $(Self::$variant(params) => serde_json::to_value(params),)*
                }
            }
        }

        impl TryFrom<JSONRPCNotification> for ServerNotification {
            type Error = serde_json::Error;

            fn try_from(value: JSONRPCNotification) -> Result<Self, serde_json::Error> {
                serde_json::from_value(serde_json::to_value(value)?)
            }
        }

        #[allow(clippy::vec_init_then_push)]
        pub fn export_server_notification_schemas(
            out_dir: &::std::path::Path,
        ) -> ::anyhow::Result<Vec<GeneratedSchema>> {
            let mut schemas = Vec::new();
            $(schemas.push(crate::export::write_json_schema::<$payload>(out_dir, stringify!($payload))?);)*
            Ok(schemas)
        }
    };
}
/// Notifications sent from the client to the server.
macro_rules! client_notification_definitions {
    (
        $(
            $(#[$variant_meta:meta])*
            $variant:ident $( ( $payload:ty ) )?
        ),* $(,)?
    ) => {
        #[derive(Serialize, Deserialize, Debug, Clone, JsonSchema, TS, Display)]
        #[serde(tag = "method", content = "params", rename_all = "camelCase")]
        #[strum(serialize_all = "camelCase")]
        pub enum ClientNotification {
            $(
                $(#[$variant_meta])*
                $variant $( ( $payload ) )?,
            )*
        }

        pub fn export_client_notification_schemas(
            _out_dir: &::std::path::Path,
        ) -> ::anyhow::Result<Vec<GeneratedSchema>> {
            let schemas = Vec::new();
            $( $(schemas.push(crate::export::write_json_schema::<$payload>(_out_dir, stringify!($payload))?);)? )*
            Ok(schemas)
        }
    };
}

impl TryFrom<JSONRPCRequest> for ServerRequest {
    type Error = serde_json::Error;

    fn try_from(value: JSONRPCRequest) -> Result<Self, Self::Error> {
        serde_json::from_value(serde_json::to_value(value)?)
    }
}

server_request_definitions! {
    /// App-gateway requests
    /// Sent when approval is requested for a specific command execution.
    /// This request is used for Turns started via turn/start.
    CommandExecutionRequestApproval => "item/commandExecution/requestApproval" {
        params: api::CommandExecutionRequestApprovalParams,
        response: api::CommandExecutionRequestApprovalResponse,
    },

    /// Sent when approval is requested for a specific file change.
    /// This request is used for Turns started via turn/start.
    FileChangeRequestApproval => "item/fileChange/requestApproval" {
        params: api::FileChangeRequestApprovalParams,
        response: api::FileChangeRequestApprovalResponse,
    },

    /// EXPERIMENTAL - Request input from the user for a tool call.
    ToolRequestUserInput => "item/tool/requestUserInput" {
        params: api::ToolRequestUserInputParams,
        response: api::ToolRequestUserInputResponse,
    },

    /// Request input for an MCP server elicitation.
    McpServerElicitationRequest => "mcpServer/elicitation/request" {
        params: api::McpServerElicitationRequestParams,
        response: api::McpServerElicitationRequestResponse,
    },

    /// Request approval for additional permissions from the user.
    PermissionsRequestApproval => "item/permissions/requestApproval" {
        params: api::PermissionsRequestApprovalParams,
        response: api::PermissionsRequestApprovalResponse,
    },

    /// Execute a dynamic tool call on the client.
    DynamicToolCall => "item/tool/call" {
        params: api::DynamicToolCallParams,
        response: api::DynamicToolCallResponse,
    },

    ChatgptAuthTokensRefresh => "account/chatgptAuthTokens/refresh" {
        params: api::ChatgptAuthTokensRefreshParams,
        response: api::ChatgptAuthTokensRefreshResponse,
    },
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct FuzzyFileSearchParams {
    pub query: String,
    pub roots: Vec<String>,
    // if provided, will cancel any previous request that used the same value
    pub cancellation_token: Option<String>,
}

/// Superset of [`praxis_file_search::FileMatch`]
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema, TS)]
pub struct FuzzyFileSearchResult {
    pub root: String,
    pub path: String,
    pub match_type: FuzzyFileSearchMatchType,
    pub file_name: String,
    pub score: u32,
    pub indices: Option<Vec<u32>>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub enum FuzzyFileSearchMatchType {
    File,
    Directory,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema, TS)]
pub struct FuzzyFileSearchResponse {
    pub files: Vec<FuzzyFileSearchResult>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct FuzzyFileSearchSessionStartParams {
    pub session_id: String,
    pub roots: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema, TS, Default)]
pub struct FuzzyFileSearchSessionStartResponse {}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct FuzzyFileSearchSessionUpdateParams {
    pub session_id: String,
    pub query: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema, TS, Default)]
pub struct FuzzyFileSearchSessionUpdateResponse {}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct FuzzyFileSearchSessionStopParams {
    pub session_id: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema, TS, Default)]
pub struct FuzzyFileSearchSessionStopResponse {}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct FuzzyFileSearchSessionUpdatedNotification {
    pub session_id: String,
    pub query: String,
    pub files: Vec<FuzzyFileSearchResult>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct FuzzyFileSearchSessionCompletedNotification {
    pub session_id: String,
}

server_notification_definitions! {
    /// NEW NOTIFICATIONS
    Error => "error" (api::ErrorNotification),
    ThreadStarted => "thread/started" (api::ThreadStartedNotification),
    ThreadStatusChanged => "thread/status/changed" (api::ThreadStatusChangedNotification),
    ThreadArchived => "thread/archived" (api::ThreadArchivedNotification),
    ThreadUnarchived => "thread/unarchived" (api::ThreadUnarchivedNotification),
    ThreadClosed => "thread/closed" (api::ThreadClosedNotification),
    SkillsChanged => "skills/changed" (api::SkillsChangedNotification),
    ThreadNameUpdated => "thread/name/updated" (api::ThreadNameUpdatedNotification),
    ThreadTokenUsageUpdated => "thread/tokenUsage/updated" (api::ThreadTokenUsageUpdatedNotification),
    #[experimental("thread/control/changed")]
    ThreadControlChanged => "thread/control/changed" (api::ThreadControlChangedNotification),
    ThreadGoalUpdated => "thread/goal/updated" (api::ThreadGoalUpdatedNotification),
    ThreadGoalCleared => "thread/goal/cleared" (api::ThreadGoalClearedNotification),
    ThreadHeartbeatUpdated => "thread/heartbeat/updated" (api::ThreadHeartbeatUpdatedNotification),
    AutomationRunUpdated => "automation/run/updated" (api::AutomationRunUpdatedNotification),
    ThreadModelChanged => "thread/model/changed" (api::ThreadModelChangedNotification),
    TurnStarted => "turn/started" (api::TurnStartedNotification),
    HookStarted => "hook/started" (api::HookStartedNotification),
    TurnCompleted => "turn/completed" (api::TurnCompletedNotification),
    HookCompleted => "hook/completed" (api::HookCompletedNotification),
    TurnDiffUpdated => "turn/diff/updated" (api::TurnDiffUpdatedNotification),
    TurnPlanUpdated => "turn/plan/updated" (api::TurnPlanUpdatedNotification),
    ItemStarted => "item/started" (api::ItemStartedNotification),
    ItemGuardianApprovalReviewStarted => "item/autoApprovalReview/started" (api::ItemGuardianApprovalReviewStartedNotification),
    ItemGuardianApprovalReviewCompleted => "item/autoApprovalReview/completed" (api::ItemGuardianApprovalReviewCompletedNotification),
    ItemCompleted => "item/completed" (api::ItemCompletedNotification),
    /// This event is internal-only. Used by Praxis Cloud.
    RawResponseItemCompleted => "rawResponseItem/completed" (api::RawResponseItemCompletedNotification),
    AgentMessageDelta => "item/agentMessage/delta" (api::AgentMessageDeltaNotification),
    /// EXPERIMENTAL - proposed plan streaming deltas for plan items.
    PlanDelta => "item/plan/delta" (api::PlanDeltaNotification),
    /// Stream base64-encoded stdout/stderr chunks for a running `command/exec` session.
    CommandExecOutputDelta => "command/exec/outputDelta" (api::CommandExecOutputDeltaNotification),
    CommandExecutionOutputDelta => "item/commandExecution/outputDelta" (api::CommandExecutionOutputDeltaNotification),
    TerminalInteraction => "item/commandExecution/terminalInteraction" (api::TerminalInteractionNotification),
    FileChangeOutputDelta => "item/fileChange/outputDelta" (api::FileChangeOutputDeltaNotification),
    ServerRequestResolved => "serverRequest/resolved" (api::ServerRequestResolvedNotification),
    McpToolCallProgress => "item/mcpToolCall/progress" (api::McpToolCallProgressNotification),
    McpServerOauthLoginCompleted => "mcpServer/oauthLogin/completed" (api::McpServerOauthLoginCompletedNotification),
    McpServerStatusUpdated => "mcpServer/startupStatus/updated" (api::McpServerStatusUpdatedNotification),
    AccountUpdated => "account/updated" (api::AccountUpdatedNotification),
    AccountRateLimitsUpdated => "account/rateLimits/updated" (api::AccountRateLimitsUpdatedNotification),
    AppListUpdated => "app/list/updated" (api::AppListUpdatedNotification),
    FsChanged => "fs/changed" (api::FsChangedNotification),
    ReasoningSummaryTextDelta => "item/reasoning/summaryTextDelta" (api::ReasoningSummaryTextDeltaNotification),
    ReasoningSummaryPartAdded => "item/reasoning/summaryPartAdded" (api::ReasoningSummaryPartAddedNotification),
    ReasoningTextDelta => "item/reasoning/textDelta" (api::ReasoningTextDeltaNotification),
    ModelRerouted => "model/rerouted" (api::ModelReroutedNotification),
    DeprecationNotice => "deprecationNotice" (api::DeprecationNoticeNotification),
    ConfigWarning => "configWarning" (api::ConfigWarningNotification),
    FuzzyFileSearchSessionUpdated => "fuzzyFileSearch/sessionUpdated" (FuzzyFileSearchSessionUpdatedNotification),
    FuzzyFileSearchSessionCompleted => "fuzzyFileSearch/sessionCompleted" (FuzzyFileSearchSessionCompletedNotification),
    #[experimental("thread/realtime/started")]
    ThreadRealtimeStarted => "thread/realtime/started" (api::ThreadRealtimeStartedNotification),
    #[experimental("thread/realtime/itemAdded")]
    ThreadRealtimeItemAdded => "thread/realtime/itemAdded" (api::ThreadRealtimeItemAddedNotification),
    #[experimental("thread/realtime/transcriptUpdated")]
    ThreadRealtimeTranscriptUpdated => "thread/realtime/transcriptUpdated" (api::ThreadRealtimeTranscriptUpdatedNotification),
    #[experimental("thread/realtime/outputAudio/delta")]
    ThreadRealtimeOutputAudioDelta => "thread/realtime/outputAudio/delta" (api::ThreadRealtimeOutputAudioDeltaNotification),
    #[experimental("thread/realtime/error")]
    ThreadRealtimeError => "thread/realtime/error" (api::ThreadRealtimeErrorNotification),
    #[experimental("thread/realtime/closed")]
    ThreadRealtimeClosed => "thread/realtime/closed" (api::ThreadRealtimeClosedNotification),

    /// Notifies the user of world-writable directories on Windows, which cannot be protected by the sandbox.
    WindowsWorldWritableWarning => "windows/worldWritableWarning" (api::WindowsWorldWritableWarningNotification),
    WindowsSandboxSetupCompleted => "windowsSandbox/setupCompleted" (api::WindowsSandboxSetupCompletedNotification),

    #[serde(rename = "account/login/completed")]
    #[ts(rename = "account/login/completed")]
    #[strum(serialize = "account/login/completed")]
    AccountLoginCompleted(api::AccountLoginCompletedNotification),

}

client_notification_definitions! {
    Initialized,
}

#[cfg(test)]
mod tests;
