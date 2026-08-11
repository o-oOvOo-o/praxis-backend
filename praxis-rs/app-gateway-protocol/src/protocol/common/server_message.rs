use super::FuzzyFileSearchSessionCompletedNotification;
use super::FuzzyFileSearchSessionUpdatedNotification;
use crate::JSONRPCNotification;
use crate::JSONRPCRequest;
use crate::RequestId;
use crate::export::GeneratedSchema;
use crate::protocol::api;
use praxis_experimental_api_macros::ExperimentalApi;
use schemars::JsonSchema;
use serde::Deserialize;
use serde::Serialize;
use std::path::Path;
use strum_macros::Display;
use ts_rs::TS;

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

            /// Returns whether transport backpressure must never discard this notification.
            pub fn requires_lossless_delivery(&self) -> bool {
                matches!(
                    self,
                    Self::Error(_)
                        | Self::ThreadStarted(_)
                        | Self::ThreadStatusChanged(_)
                        | Self::ThreadArchived(_)
                        | Self::ThreadUnarchived(_)
                        | Self::ThreadClosed(_)
                        | Self::ThreadNameUpdated(_)
                        | Self::ThreadControlChanged(_)
                        | Self::ThreadGoalUpdated(_)
                        | Self::ThreadGoalCleared(_)
                        | Self::ThreadHeartbeatUpdated(_)
                        | Self::ThreadSelfworkUpdated(_)
                        | Self::WorkspaceChangeUpdated(_)
                        | Self::AutomationRunUpdated(_)
                        | Self::ThreadModelChanged(_)
                        | Self::ThreadPermissionsChanged(_)
                        | Self::TurnStarted(_)
                        | Self::TurnCompleted(_)
                        | Self::ItemStarted(_)
                        | Self::ItemCompleted(_)
                        | Self::AgentMessageDelta(_)
                        | Self::PlanDelta(_)
                        | Self::ReasoningSummaryTextDelta(_)
                        | Self::ReasoningSummaryPartAdded(_)
                        | Self::ReasoningTextDelta(_)
                        | Self::TerminalInteraction(_)
                        | Self::HookStarted(_)
                        | Self::HookCompleted(_)
                        | Self::ItemGuardianApprovalReviewStarted(_)
                        | Self::ItemGuardianApprovalReviewCompleted(_)
                        | Self::ServerRequestResolved(_)
                        | Self::SkillsChanged(_)
                        | Self::ModelRerouted(_)
                        | Self::AccountUpdated(_)
                        | Self::AccountLoginCompleted(_)
                        | Self::McpServerOauthLoginCompleted(_)
                        | Self::ThreadRealtimeStarted(_)
                        | Self::ThreadRealtimeError(_)
                        | Self::ThreadRealtimeClosed(_)
                )
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
    ThreadSelfworkUpdated => "thread/selfwork/updated" (api::ThreadSelfworkUpdatedNotification),
    WorkspaceChangeUpdated => "workspace/change/updated" (api::WorkspaceChangeUpdatedNotification),
    AutomationRunUpdated => "automation/run/updated" (api::AutomationRunUpdatedNotification),
    ThreadModelChanged => "thread/model/changed" (api::ThreadModelChangedNotification),
    ThreadPermissionsChanged => "thread/permissions/changed" (api::ThreadPermissionsChangedNotification),
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
