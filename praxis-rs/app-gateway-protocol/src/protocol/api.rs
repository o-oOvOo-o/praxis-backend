use std::collections::BTreeMap;
use std::collections::HashMap;
use std::path::PathBuf;

use crate::RequestId;
use crate::protocol::common::AuthMode;
use praxis_experimental_api_macros::ExperimentalApi;
use praxis_protocol::account::PlanType;
#[cfg(test)]
use praxis_protocol::approvals::ElicitationRequest as CoreElicitationRequest;
use praxis_protocol::approvals::ExecPolicyAmendment as CoreExecPolicyAmendment;
use praxis_protocol::approvals::GuardianAssessmentAction as CoreGuardianAssessmentAction;
use praxis_protocol::approvals::GuardianCommandSource as CoreGuardianCommandSource;
use praxis_protocol::approvals::NetworkApprovalContext as CoreNetworkApprovalContext;
use praxis_protocol::approvals::NetworkApprovalProtocol as CoreNetworkApprovalProtocol;
use praxis_protocol::approvals::NetworkPolicyAmendment as CoreNetworkPolicyAmendment;
use praxis_protocol::approvals::NetworkPolicyRuleAction as CoreNetworkPolicyRuleAction;
use praxis_protocol::config_types::ApprovalsReviewer as CoreApprovalsReviewer;
use praxis_protocol::config_types::CollaborationMode;
use praxis_protocol::config_types::CollaborationModeMask as CoreCollaborationModeMask;
use praxis_protocol::config_types::ForcedLoginMethod;
use praxis_protocol::config_types::ModeKind;
use praxis_protocol::config_types::Personality;
use praxis_protocol::config_types::ReasoningSummary;
use praxis_protocol::config_types::SandboxMode as CoreSandboxMode;
use praxis_protocol::config_types::ServiceTier;
use praxis_protocol::config_types::Verbosity;
use praxis_protocol::config_types::WebSearchMode;
use praxis_protocol::config_types::WebSearchToolConfig;
use praxis_protocol::items::AgentMessageContent as CoreAgentMessageContent;
use praxis_protocol::items::TurnItem as CoreTurnItem;
use praxis_protocol::mcp::Resource as McpResource;
use praxis_protocol::mcp::ResourceTemplate as McpResourceTemplate;
use praxis_protocol::mcp::Tool as McpTool;
use praxis_protocol::memory_citation::MemoryCitation as CoreMemoryCitation;
use praxis_protocol::memory_citation::MemoryCitationEntry as CoreMemoryCitationEntry;
use praxis_protocol::models::FileSystemPermissions as CoreFileSystemPermissions;
use praxis_protocol::models::MessagePhase;
use praxis_protocol::models::NetworkPermissions as CoreNetworkPermissions;
use praxis_protocol::models::PermissionProfile as CorePermissionProfile;
use praxis_protocol::models::ResponseItem;
use praxis_protocol::openai_models::InputModality;
use praxis_protocol::openai_models::ModelAvailabilityNux as CoreModelAvailabilityNux;
use praxis_protocol::openai_models::ReasoningEffort;
use praxis_protocol::openai_models::default_input_modalities;
use praxis_protocol::parse_command::ParsedCommand as CoreParsedCommand;
use praxis_protocol::plan_tool::PlanItemArg as CorePlanItemArg;
use praxis_protocol::plan_tool::StepStatus as CorePlanStepStatus;
use praxis_protocol::protocol::AgentStatus as CoreAgentStatus;
use praxis_protocol::protocol::AskForApproval as CoreAskForApproval;
use praxis_protocol::protocol::CreditsSnapshot as CoreCreditsSnapshot;
use praxis_protocol::protocol::ExecCommandSource as CoreExecCommandSource;
use praxis_protocol::protocol::ExecCommandStatus as CoreExecCommandStatus;
use praxis_protocol::protocol::GranularApprovalConfig as CoreGranularApprovalConfig;
use praxis_protocol::protocol::GuardianRiskLevel as CoreGuardianRiskLevel;
use praxis_protocol::protocol::HookEventName as CoreHookEventName;
use praxis_protocol::protocol::HookExecutionMode as CoreHookExecutionMode;
use praxis_protocol::protocol::HookHandlerType as CoreHookHandlerType;
use praxis_protocol::protocol::HookOutputEntry as CoreHookOutputEntry;
use praxis_protocol::protocol::HookOutputEntryKind as CoreHookOutputEntryKind;
use praxis_protocol::protocol::HookRunStatus as CoreHookRunStatus;
use praxis_protocol::protocol::HookRunSummary as CoreHookRunSummary;
use praxis_protocol::protocol::HookScope as CoreHookScope;
use praxis_protocol::protocol::ModelRerouteReason as CoreModelRerouteReason;
use praxis_protocol::protocol::NonSteerableTurnKind as CoreNonSteerableTurnKind;
use praxis_protocol::protocol::PatchApplyStatus as CorePatchApplyStatus;
use praxis_protocol::protocol::PraxisErrorInfo as CorePraxisErrorInfo;
use praxis_protocol::protocol::RateLimitSnapshot as CoreRateLimitSnapshot;
use praxis_protocol::protocol::RateLimitWindow as CoreRateLimitWindow;
use praxis_protocol::protocol::ReadOnlyAccess as CoreReadOnlyAccess;
use praxis_protocol::protocol::RealtimeAudioFrame as CoreRealtimeAudioFrame;
use praxis_protocol::protocol::RealtimeConversationVersion;
use praxis_protocol::protocol::ReviewDecision as CoreReviewDecision;
use praxis_protocol::protocol::SessionSource as CoreSessionSource;
use praxis_protocol::protocol::SkillDependencies as CoreSkillDependencies;
use praxis_protocol::protocol::SkillErrorInfo as CoreSkillErrorInfo;
use praxis_protocol::protocol::SkillInterface as CoreSkillInterface;
use praxis_protocol::protocol::SkillMetadata as CoreSkillMetadata;
use praxis_protocol::protocol::SkillScope as CoreSkillScope;
use praxis_protocol::protocol::SkillToolDependency as CoreSkillToolDependency;
use praxis_protocol::protocol::SubAgentSource as CoreSubAgentSource;
use praxis_protocol::protocol::ThreadGoal as CoreThreadGoal;
use praxis_protocol::protocol::ThreadGoalStatus as CoreThreadGoalStatus;
use praxis_protocol::protocol::ThreadHeartbeat as CoreThreadHeartbeat;
use praxis_protocol::protocol::TokenUsage as CoreTokenUsage;
use praxis_protocol::protocol::TokenUsageInfo as CoreTokenUsageInfo;
use praxis_protocol::request_permissions::PermissionGrantScope as CorePermissionGrantScope;
use praxis_protocol::request_permissions::RequestPermissionProfile as CoreRequestPermissionProfile;
use praxis_protocol::user_input::ByteRange as CoreByteRange;
use praxis_protocol::user_input::TextElement as CoreTextElement;
use praxis_protocol::user_input::UserInput as CoreUserInput;
use praxis_utils_absolute_path::AbsolutePathBuf;
use schemars::JsonSchema;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value as JsonValue;
use serde_with::serde_as;
use thiserror::Error;
use ts_rs::TS;

pub use praxis_protocol::apps::AppBranding;
pub use praxis_protocol::apps::AppInfo;
pub use praxis_protocol::apps::AppMetadata;
pub use praxis_protocol::apps::AppReview;
pub use praxis_protocol::apps::AppScreenshot;
pub use praxis_protocol::config_layers::ConfigLayer;
pub use praxis_protocol::config_layers::ConfigLayerMetadata;
pub use praxis_protocol::config_layers::ConfigLayerSource;
pub use praxis_protocol::dynamic_tools::DynamicToolSpec;
pub use praxis_protocol::fs::FsCopyParams;
pub use praxis_protocol::fs::FsCopyResponse;
pub use praxis_protocol::fs::FsCreateDirectoryParams;
pub use praxis_protocol::fs::FsCreateDirectoryResponse;
pub use praxis_protocol::fs::FsGetMetadataParams;
pub use praxis_protocol::fs::FsGetMetadataResponse;
pub use praxis_protocol::fs::FsReadDirectoryEntry;
pub use praxis_protocol::fs::FsReadDirectoryParams;
pub use praxis_protocol::fs::FsReadDirectoryResponse;
pub use praxis_protocol::fs::FsReadFileParams;
pub use praxis_protocol::fs::FsReadFileResponse;
pub use praxis_protocol::fs::FsRemoveParams;
pub use praxis_protocol::fs::FsRemoveResponse;
pub use praxis_protocol::fs::FsWriteFileParams;
pub use praxis_protocol::fs::FsWriteFileResponse;
pub use praxis_protocol::mcp_elicitation::McpElicitationArrayType;
pub use praxis_protocol::mcp_elicitation::McpElicitationBooleanSchema;
pub use praxis_protocol::mcp_elicitation::McpElicitationBooleanType;
pub use praxis_protocol::mcp_elicitation::McpElicitationConstOption;
pub use praxis_protocol::mcp_elicitation::McpElicitationEnumSchema;
pub use praxis_protocol::mcp_elicitation::McpElicitationLegacyTitledEnumSchema;
pub use praxis_protocol::mcp_elicitation::McpElicitationMultiSelectEnumSchema;
pub use praxis_protocol::mcp_elicitation::McpElicitationNumberSchema;
pub use praxis_protocol::mcp_elicitation::McpElicitationNumberType;
pub use praxis_protocol::mcp_elicitation::McpElicitationObjectType;
pub use praxis_protocol::mcp_elicitation::McpElicitationPrimitiveSchema;
pub use praxis_protocol::mcp_elicitation::McpElicitationSchema;
pub use praxis_protocol::mcp_elicitation::McpElicitationSingleSelectEnumSchema;
pub use praxis_protocol::mcp_elicitation::McpElicitationStringFormat;
pub use praxis_protocol::mcp_elicitation::McpElicitationStringSchema;
pub use praxis_protocol::mcp_elicitation::McpElicitationStringType;
pub use praxis_protocol::mcp_elicitation::McpElicitationTitledEnumItems;
pub use praxis_protocol::mcp_elicitation::McpElicitationTitledMultiSelectEnumSchema;
pub use praxis_protocol::mcp_elicitation::McpElicitationTitledSingleSelectEnumSchema;
pub use praxis_protocol::mcp_elicitation::McpElicitationUntitledEnumItems;
pub use praxis_protocol::mcp_elicitation::McpElicitationUntitledMultiSelectEnumSchema;
pub use praxis_protocol::mcp_elicitation::McpElicitationUntitledSingleSelectEnumSchema;
pub use praxis_protocol::mcp_elicitation::McpServerElicitationAction;
pub use praxis_protocol::mcp_elicitation::McpServerElicitationRequest;
pub use praxis_protocol::mcp_elicitation::McpServerElicitationRequestParams;
pub use praxis_protocol::protocol::NetworkAccess;

// Macro to declare a camelCased API enum mirroring a core enum which
// tends to use either snake_case or kebab-case.
macro_rules! api_enum_from_core {
    (
        $(#[$enum_meta:meta])*
        pub enum $Name:ident from $Src:path {
            $( $(#[$variant_meta:meta])* $Variant:ident ),+ $(,)?
        }
    ) => {
        #[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, JsonSchema, TS)]
        $(#[$enum_meta])*
        #[serde(rename_all = "camelCase")]
        pub enum $Name {
            $( $(#[$variant_meta])* $Variant ),+
        }

        impl $Name {
            pub fn to_core(self) -> $Src {
                match self { $( $Name::$Variant => <$Src>::$Variant ),+ }
            }
        }

        impl From<$Src> for $Name {
            fn from(value: $Src) -> Self {
                match value { $( <$Src>::$Variant => $Name::$Variant ),+ }
            }
        }
    };
}

mod account_model_features;
mod approvals_permissions;
mod apps_mcp_fs;
mod automation;
mod command_exec;
mod config;
mod core_types;
mod initialize;
mod notifications;
mod realtime;
mod skills_plugins;
#[cfg(test)]
mod tests;
mod thread_common;
mod thread_entities;
mod thread_requests;
mod turn_items;

pub use account_model_features::*;
pub use approvals_permissions::*;
pub use apps_mcp_fs::*;
pub use automation::*;
pub use command_exec::*;
pub use config::*;
pub use core_types::*;
pub use initialize::*;
pub use notifications::*;
pub use realtime::*;
pub use skills_plugins::*;
pub use thread_common::*;
pub use thread_entities::*;
pub use thread_requests::*;
pub use turn_items::*;
