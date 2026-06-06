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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
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
    #[experimental("thread/control/acquire")]
    ThreadControlAcquire => "thread/control/acquire" {
        params: api::ThreadControlAcquireParams,
        response: api::ThreadControlAcquireResponse,
    },
    #[experimental("thread/control/release")]
    ThreadControlRelease => "thread/control/release" {
        params: api::ThreadControlReleaseParams,
        response: api::ThreadControlReleaseResponse,
    },
    SkillsList => "skills/list" {
        params: api::SkillsListParams,
        response: api::SkillsListResponse,
    },
    PluginList => "plugin/list" {
        params: api::PluginListParams,
        response: api::PluginListResponse,
    },
    PluginRead => "plugin/read" {
        params: api::PluginReadParams,
        response: api::PluginReadResponse,
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
mod tests {
    use super::*;
    use anyhow::Result;
    use praxis_protocol::ThreadId;
    use praxis_protocol::account::PlanType;
    use praxis_protocol::protocol::RealtimeConversationVersion;
    use praxis_utils_absolute_path::AbsolutePathBuf;
    use pretty_assertions::assert_eq;
    use serde_json::json;
    use std::path::PathBuf;

    fn absolute_path_string(path: &str) -> String {
        let trimmed = path.trim_start_matches('/');
        if cfg!(windows) {
            format!(r"C:\{}", trimmed.replace('/', "\\"))
        } else {
            format!("/{trimmed}")
        }
    }

    fn absolute_path(path: &str) -> AbsolutePathBuf {
        AbsolutePathBuf::from_absolute_path(absolute_path_string(path)).expect("absolute path")
    }

    #[test]
    fn serialize_initialize_with_opt_out_notification_methods() -> Result<()> {
        let request = ClientRequest::Initialize {
            request_id: RequestId::Integer(42),
            params: api::InitializeParams {
                client_info: api::ClientInfo {
                    name: "praxis_vscode".to_string(),
                    title: Some("Praxis VS Code Extension".to_string()),
                    version: "0.1.0".to_string(),
                },
                capabilities: Some(api::InitializeCapabilities {
                    experimental_api: true,
                    opt_out_notification_methods: Some(vec![
                        "thread/started".to_string(),
                        "item/agentMessage/delta".to_string(),
                    ]),
                }),
            },
        };

        assert_eq!(
            json!({
                "method": "initialize",
                "id": 42,
                "params": {
                    "clientInfo": {
                        "name": "praxis_vscode",
                        "title": "Praxis VS Code Extension",
                        "version": "0.1.0"
                    },
                    "capabilities": {
                        "experimentalApi": true,
                        "optOutNotificationMethods": [
                            "thread/started",
                            "item/agentMessage/delta"
                        ]
                    }
                }
            }),
            serde_json::to_value(&request)?,
        );
        Ok(())
    }

    #[test]
    fn deserialize_initialize_with_opt_out_notification_methods() -> Result<()> {
        let request: ClientRequest = serde_json::from_value(json!({
            "method": "initialize",
            "id": 42,
            "params": {
                "clientInfo": {
                    "name": "praxis_vscode",
                    "title": "Praxis VS Code Extension",
                    "version": "0.1.0"
                },
                "capabilities": {
                    "experimentalApi": true,
                    "optOutNotificationMethods": [
                        "thread/started",
                        "item/agentMessage/delta"
                    ]
                }
            }
        }))?;

        assert_eq!(
            request,
            ClientRequest::Initialize {
                request_id: RequestId::Integer(42),
                params: api::InitializeParams {
                    client_info: api::ClientInfo {
                        name: "praxis_vscode".to_string(),
                        title: Some("Praxis VS Code Extension".to_string()),
                        version: "0.1.0".to_string(),
                    },
                    capabilities: Some(api::InitializeCapabilities {
                        experimental_api: true,
                        opt_out_notification_methods: Some(vec![
                            "thread/started".to_string(),
                            "item/agentMessage/delta".to_string(),
                        ]),
                    }),
                },
            }
        );
        Ok(())
    }

    #[test]
    fn conversation_id_serializes_as_plain_string() -> Result<()> {
        let id = ThreadId::from_string("67e55044-10b1-426f-9247-bb680e5fe0c8")?;

        assert_eq!(
            json!("67e55044-10b1-426f-9247-bb680e5fe0c8"),
            serde_json::to_value(id)?
        );
        Ok(())
    }

    #[test]
    fn conversation_id_deserializes_from_plain_string() -> Result<()> {
        let id: ThreadId = serde_json::from_value(json!("67e55044-10b1-426f-9247-bb680e5fe0c8"))?;

        assert_eq!(
            ThreadId::from_string("67e55044-10b1-426f-9247-bb680e5fe0c8")?,
            id,
        );
        Ok(())
    }

    #[test]
    fn serialize_client_notification() -> Result<()> {
        let notification = ClientNotification::Initialized;
        // Note there is no "params" field for this notification.
        assert_eq!(
            json!({
                "method": "initialized",
            }),
            serde_json::to_value(&notification)?,
        );
        Ok(())
    }

    #[test]
    fn serialize_chatgpt_auth_tokens_refresh_request() -> Result<()> {
        let request = ServerRequest::ChatgptAuthTokensRefresh {
            request_id: RequestId::Integer(8),
            params: api::ChatgptAuthTokensRefreshParams {
                reason: api::ChatgptAuthTokensRefreshReason::Unauthorized,
                previous_account_id: Some("org-123".to_string()),
            },
        };
        assert_eq!(
            json!({
                "method": "account/chatgptAuthTokens/refresh",
                "id": 8,
                "params": {
                    "reason": "unauthorized",
                    "previousAccountId": "org-123"
                }
            }),
            serde_json::to_value(&request)?,
        );
        Ok(())
    }

    #[test]
    fn serialize_mcp_server_elicitation_request() -> Result<()> {
        let requested_schema: api::McpElicitationSchema = serde_json::from_value(json!({
            "type": "object",
            "properties": {
                "confirmed": {
                    "type": "boolean"
                }
            },
            "required": ["confirmed"]
        }))?;
        let params = api::McpServerElicitationRequestParams {
            thread_id: "thr_123".to_string(),
            turn_id: Some("turn_123".to_string()),
            server_name: "praxis_apps".to_string(),
            request: api::McpServerElicitationRequest::Form {
                meta: None,
                message: "Allow this request?".to_string(),
                requested_schema,
            },
        };
        let request = ServerRequest::McpServerElicitationRequest {
            request_id: RequestId::Integer(9),
            params: params.clone(),
        };

        assert_eq!(
            json!({
                "method": "mcpServer/elicitation/request",
                "id": 9,
                "params": {
                    "threadId": "thr_123",
                    "turnId": "turn_123",
                    "serverName": "praxis_apps",
                    "mode": "form",
                    "_meta": null,
                    "message": "Allow this request?",
                    "requestedSchema": {
                        "type": "object",
                        "properties": {
                            "confirmed": {
                                "type": "boolean"
                            }
                        },
                        "required": ["confirmed"]
                    }
                }
            }),
            serde_json::to_value(&request)?,
        );

        let payload = ServerRequestPayload::McpServerElicitationRequest(params);
        assert_eq!(request.id(), &RequestId::Integer(9));
        assert_eq!(payload.request_with_id(RequestId::Integer(9)), request);
        Ok(())
    }

    #[test]
    fn serialize_get_account_rate_limits() -> Result<()> {
        let request = ClientRequest::GetAccountRateLimits {
            request_id: RequestId::Integer(1),
            params: None,
        };
        assert_eq!(request.id(), &RequestId::Integer(1));
        assert_eq!(request.method(), "account/rateLimits/read");
        assert_eq!(
            json!({
                "method": "account/rateLimits/read",
                "id": 1,
            }),
            serde_json::to_value(&request)?,
        );
        Ok(())
    }

    #[test]
    fn serialize_client_response() -> Result<()> {
        let response = ClientResponse::ThreadStart {
            request_id: RequestId::Integer(7),
            response: api::ThreadStartResponse {
                thread: api::Thread {
                    id: "67e55044-10b1-426f-9247-bb680e5fe0c8".to_string(),
                    preview: "first prompt".to_string(),
                    summary: None,
                    ephemeral: true,
                    model_provider: "openai".to_string(),
                    model: Some("gpt-5".to_string()),
                    created_at: 1,
                    updated_at: 2,
                    status: api::ThreadStatus::Idle,
                    path: None,
                    cwd: PathBuf::from("/tmp"),
                    cli_version: "0.0.0".to_string(),
                    source: api::SessionSource::Exec,
                    agent_base_name: None,
                    agent_title: None,
                    agent_display_name: None,
                    agent_role: None,
                    git_info: None,
                    name: None,
                    total_cost_usd: None,
                    last_cost_usd: None,
                    token_usage: None,
                    control_state: None,
                    selfwork_plan_path: None,
                    turns: Vec::new(),
                },
                model: "gpt-5".to_string(),
                model_provider: "openai".to_string(),
                service_tier: None,
                cwd: PathBuf::from("/tmp"),
                approval_policy: api::AskForApproval::OnFailure,
                approvals_reviewer: api::ApprovalsReviewer::User,
                sandbox: api::SandboxPolicy::DangerFullAccess,
                reasoning_effort: None,
            },
        };

        assert_eq!(response.id(), &RequestId::Integer(7));
        assert_eq!(response.method(), "thread/start");
        assert_eq!(
            json!({
                "method": "thread/start",
                "id": 7,
                "response": {
                    "thread": {
                        "id": "67e55044-10b1-426f-9247-bb680e5fe0c8",
                        "preview": "first prompt",
                        "ephemeral": true,
                        "modelProvider": "openai",
                        "createdAt": 1,
                        "updatedAt": 2,
                        "status": {
                            "type": "idle"
                        },
                        "path": null,
                        "cwd": "/tmp",
                        "cliVersion": "0.0.0",
                        "source": "exec",
                        "agentNickname": null,
                        "agentRole": null,
                        "gitInfo": null,
                        "name": null,
                        "turns": []
                    },
                    "model": "gpt-5",
                    "modelProvider": "openai",
                    "serviceTier": null,
                    "cwd": "/tmp",
                    "approvalPolicy": "on-failure",
                    "approvalsReviewer": "user",
                    "sandbox": {
                        "type": "dangerFullAccess"
                    },
                    "reasoningEffort": null
                }
            }),
            serde_json::to_value(&response)?,
        );
        Ok(())
    }

    #[test]
    fn serialize_config_requirements_read() -> Result<()> {
        let request = ClientRequest::ConfigRequirementsRead {
            request_id: RequestId::Integer(1),
            params: None,
        };
        assert_eq!(
            json!({
                "method": "configRequirements/read",
                "id": 1,
            }),
            serde_json::to_value(&request)?,
        );
        Ok(())
    }

    #[test]
    fn serialize_account_login_api_key() -> Result<()> {
        let request = ClientRequest::LoginAccount {
            request_id: RequestId::Integer(2),
            params: api::LoginAccountParams::ApiKey {
                api_key: "secret".to_string(),
            },
        };
        assert_eq!(
            json!({
                "method": "account/login/start",
                "id": 2,
                "params": {
                    "type": "apiKey",
                    "apiKey": "secret"
                }
            }),
            serde_json::to_value(&request)?,
        );
        Ok(())
    }

    #[test]
    fn serialize_account_login_chatgpt() -> Result<()> {
        let request = ClientRequest::LoginAccount {
            request_id: RequestId::Integer(3),
            params: api::LoginAccountParams::Chatgpt,
        };
        assert_eq!(
            json!({
                "method": "account/login/start",
                "id": 3,
                "params": {
                    "type": "chatgpt"
                }
            }),
            serde_json::to_value(&request)?,
        );
        Ok(())
    }

    #[test]
    fn serialize_account_login_chatgpt_device_code() -> Result<()> {
        let request = ClientRequest::LoginAccount {
            request_id: RequestId::Integer(4),
            params: api::LoginAccountParams::ChatgptDeviceCode,
        };
        assert_eq!(
            json!({
                "method": "account/login/start",
                "id": 4,
                "params": {
                    "type": "chatgptDeviceCode"
                }
            }),
            serde_json::to_value(&request)?,
        );
        Ok(())
    }

    #[test]
    fn serialize_account_logout() -> Result<()> {
        let request = ClientRequest::LogoutAccount {
            request_id: RequestId::Integer(5),
            params: None,
        };
        assert_eq!(
            json!({
                "method": "account/logout",
                "id": 5,
            }),
            serde_json::to_value(&request)?,
        );
        Ok(())
    }

    #[test]
    fn serialize_account_login_chatgpt_auth_tokens() -> Result<()> {
        let request = ClientRequest::LoginAccount {
            request_id: RequestId::Integer(6),
            params: api::LoginAccountParams::ChatgptAuthTokens {
                access_token: "access-token".to_string(),
                chatgpt_account_id: "org-123".to_string(),
                chatgpt_plan_type: Some("business".to_string()),
            },
        };
        assert_eq!(
            json!({
                "method": "account/login/start",
                "id": 6,
                "params": {
                    "type": "chatgptAuthTokens",
                    "accessToken": "access-token",
                    "chatgptAccountId": "org-123",
                    "chatgptPlanType": "business"
                }
            }),
            serde_json::to_value(&request)?,
        );
        Ok(())
    }

    #[test]
    fn serialize_get_account() -> Result<()> {
        let request = ClientRequest::GetAccount {
            request_id: RequestId::Integer(6),
            params: api::GetAccountParams {
                refresh_token: false,
            },
        };
        assert_eq!(
            json!({
                "method": "account/read",
                "id": 6,
                "params": {
                    "refreshToken": false
                }
            }),
            serde_json::to_value(&request)?,
        );
        Ok(())
    }

    #[test]
    fn account_serializes_fields_in_camel_case() -> Result<()> {
        let api_key = api::Account::ApiKey {};
        assert_eq!(
            json!({
                "type": "apiKey",
            }),
            serde_json::to_value(&api_key)?,
        );

        let chatgpt = api::Account::Chatgpt {
            email: "user@example.com".to_string(),
            plan_type: PlanType::Plus,
        };
        assert_eq!(
            json!({
                "type": "chatgpt",
                "email": "user@example.com",
                "planType": "plus",
            }),
            serde_json::to_value(&chatgpt)?,
        );

        Ok(())
    }

    #[test]
    fn serialize_list_models() -> Result<()> {
        let request = ClientRequest::ModelList {
            request_id: RequestId::Integer(6),
            params: api::ModelListParams::default(),
        };
        assert_eq!(
            json!({
                "method": "model/list",
                "id": 6,
                "params": {
                    "limit": null,
                    "cursor": null,
                    "includeHidden": null
                }
            }),
            serde_json::to_value(&request)?,
        );
        Ok(())
    }

    #[test]
    fn serialize_list_collaboration_modes() -> Result<()> {
        let request = ClientRequest::CollaborationModeList {
            request_id: RequestId::Integer(7),
            params: api::CollaborationModeListParams::default(),
        };
        assert_eq!(
            json!({
                "method": "collaborationMode/list",
                "id": 7,
                "params": {}
            }),
            serde_json::to_value(&request)?,
        );
        Ok(())
    }

    #[test]
    fn serialize_list_apps() -> Result<()> {
        let request = ClientRequest::AppsList {
            request_id: RequestId::Integer(8),
            params: api::AppsListParams::default(),
        };
        assert_eq!(
            json!({
                "method": "app/list",
                "id": 8,
                "params": {
                    "cursor": null,
                    "limit": null,
                    "threadId": null
                }
            }),
            serde_json::to_value(&request)?,
        );
        Ok(())
    }

    #[test]
    fn serialize_fs_get_metadata() -> Result<()> {
        let request = ClientRequest::FsGetMetadata {
            request_id: RequestId::Integer(9),
            params: api::FsGetMetadataParams {
                path: absolute_path("tmp/example"),
            },
        };
        assert_eq!(
            json!({
                "method": "fs/getMetadata",
                "id": 9,
                "params": {
                    "path": absolute_path_string("tmp/example")
                }
            }),
            serde_json::to_value(&request)?,
        );
        Ok(())
    }

    #[test]
    fn serialize_fs_watch() -> Result<()> {
        let request = ClientRequest::FsWatch {
            request_id: RequestId::Integer(10),
            params: api::FsWatchParams {
                path: absolute_path("tmp/repo/.git"),
            },
        };
        assert_eq!(
            json!({
                "method": "fs/watch",
                "id": 10,
                "params": {
                    "path": absolute_path_string("tmp/repo/.git")
                }
            }),
            serde_json::to_value(&request)?,
        );
        Ok(())
    }

    #[test]
    fn serialize_list_experimental_features() -> Result<()> {
        let request = ClientRequest::ExperimentalFeatureList {
            request_id: RequestId::Integer(8),
            params: api::ExperimentalFeatureListParams::default(),
        };
        assert_eq!(
            json!({
                "method": "experimentalFeature/list",
                "id": 8,
                "params": {
                    "cursor": null,
                    "limit": null
                }
            }),
            serde_json::to_value(&request)?,
        );
        Ok(())
    }

    #[test]
    fn serialize_thread_background_terminals_clean() -> Result<()> {
        let request = ClientRequest::ThreadBackgroundTerminalsClean {
            request_id: RequestId::Integer(8),
            params: api::ThreadBackgroundTerminalsCleanParams {
                thread_id: "thr_123".to_string(),
            },
        };
        assert_eq!(
            json!({
                "method": "thread/backgroundTerminals/clean",
                "id": 8,
                "params": {
                    "threadId": "thr_123"
                }
            }),
            serde_json::to_value(&request)?,
        );
        Ok(())
    }

    #[test]
    fn serialize_thread_realtime_start() -> Result<()> {
        let request = ClientRequest::ThreadRealtimeStart {
            request_id: RequestId::Integer(9),
            params: api::ThreadRealtimeStartParams {
                thread_id: "thr_123".to_string(),
                prompt: "You are on a call".to_string(),
                session_id: Some("sess_456".to_string()),
            },
        };
        assert_eq!(
            json!({
                "method": "thread/realtime/start",
                "id": 9,
                "params": {
                    "threadId": "thr_123",
                    "prompt": "You are on a call",
                    "sessionId": "sess_456"
                }
            }),
            serde_json::to_value(&request)?,
        );
        Ok(())
    }

    #[test]
    fn serialize_thread_status_changed_notification() -> Result<()> {
        let notification =
            ServerNotification::ThreadStatusChanged(api::ThreadStatusChangedNotification {
                thread_id: "thr_123".to_string(),
                status: api::ThreadStatus::Idle,
            });
        assert_eq!(
            json!({
                "method": "thread/status/changed",
                "params": {
                    "threadId": "thr_123",
                    "status": {
                        "type": "idle"
                    },
                }
            }),
            serde_json::to_value(&notification)?,
        );
        Ok(())
    }

    #[test]
    fn serialize_thread_realtime_output_audio_delta_notification() -> Result<()> {
        let notification = ServerNotification::ThreadRealtimeOutputAudioDelta(
            api::ThreadRealtimeOutputAudioDeltaNotification {
                thread_id: "thr_123".to_string(),
                audio: api::ThreadRealtimeAudioChunk {
                    data: "AQID".to_string(),
                    sample_rate: 24_000,
                    num_channels: 1,
                    samples_per_channel: Some(512),
                    item_id: None,
                },
            },
        );
        assert_eq!(
            json!({
                "method": "thread/realtime/outputAudio/delta",
                "params": {
                    "threadId": "thr_123",
                    "audio": {
                        "data": "AQID",
                        "sampleRate": 24000,
                        "numChannels": 1,
                        "samplesPerChannel": 512,
                        "itemId": null
                    }
                }
            }),
            serde_json::to_value(&notification)?,
        );
        Ok(())
    }

    #[test]
    fn mock_experimental_method_is_marked_experimental() {
        let request = ClientRequest::MockExperimentalMethod {
            request_id: RequestId::Integer(1),
            params: api::MockExperimentalMethodParams::default(),
        };
        let reason = crate::experimental_api::ExperimentalApi::experimental_reason(&request);
        assert_eq!(reason, Some("mock/experimentalMethod"));
    }
    #[test]
    fn thread_realtime_start_is_marked_experimental() {
        let request = ClientRequest::ThreadRealtimeStart {
            request_id: RequestId::Integer(1),
            params: api::ThreadRealtimeStartParams {
                thread_id: "thr_123".to_string(),
                prompt: "You are on a call".to_string(),
                session_id: None,
            },
        };
        let reason = crate::experimental_api::ExperimentalApi::experimental_reason(&request);
        assert_eq!(reason, Some("thread/realtime/start"));
    }
    #[test]
    fn thread_realtime_started_notification_is_marked_experimental() {
        let notification =
            ServerNotification::ThreadRealtimeStarted(api::ThreadRealtimeStartedNotification {
                thread_id: "thr_123".to_string(),
                session_id: Some("sess_456".to_string()),
                version: RealtimeConversationVersion::default(),
            });
        let reason = crate::experimental_api::ExperimentalApi::experimental_reason(&notification);
        assert_eq!(reason, Some("thread/realtime/started"));
    }

    #[test]
    fn thread_realtime_output_audio_delta_notification_is_marked_experimental() {
        let notification = ServerNotification::ThreadRealtimeOutputAudioDelta(
            api::ThreadRealtimeOutputAudioDeltaNotification {
                thread_id: "thr_123".to_string(),
                audio: api::ThreadRealtimeAudioChunk {
                    data: "AQID".to_string(),
                    sample_rate: 24_000,
                    num_channels: 1,
                    samples_per_channel: Some(512),
                    item_id: None,
                },
            },
        );
        let reason = crate::experimental_api::ExperimentalApi::experimental_reason(&notification);
        assert_eq!(reason, Some("thread/realtime/outputAudio/delta"));
    }

    #[test]
    fn command_execution_request_approval_additional_permissions_is_marked_experimental() {
        let params = api::CommandExecutionRequestApprovalParams {
            thread_id: "thr_123".to_string(),
            turn_id: "turn_123".to_string(),
            item_id: "call_123".to_string(),
            approval_id: None,
            reason: None,
            network_approval_context: None,
            command: Some("cat file".to_string()),
            cwd: None,
            command_actions: None,
            additional_permissions: Some(api::AdditionalPermissionProfile {
                network: None,
                file_system: Some(api::AdditionalFileSystemPermissions {
                    read: Some(vec![absolute_path("/tmp/allowed")]),
                    write: None,
                }),
            }),
            proposed_execpolicy_amendment: None,
            proposed_network_policy_amendments: None,
            available_decisions: None,
        };
        let reason = crate::experimental_api::ExperimentalApi::experimental_reason(&params);
        assert_eq!(
            reason,
            Some("item/commandExecution/requestApproval.additionalPermissions")
        );
    }
}
