//! Exercises `ChatWidget` event handling and rendering invariants.
//!
//! These tests treat the widget as the adapter between `praxis_protocol::protocol::EventMsg` inputs and
//! the TUI output. Many assertions are snapshot-based so that layout regressions and status/header
//! changes show up as stable, reviewable diffs.

pub(super) use super::*;
pub(super) use crate::app_event::AppEvent;
pub(super) use crate::app_event::ExitMode;
#[cfg(not(target_os = "linux"))]
pub(super) use crate::app_event::RealtimeAudioDeviceKind;
pub(super) use crate::app_event_sender::AppEventSender;
pub(super) use crate::app_gateway_core_conversions::app_gateway_patch_changes_to_core;
pub(super) use crate::app_gateway_core_conversions::exec_approval_request_from_params;
pub(super) use crate::app_gateway_core_conversions::request_permissions_from_params;
pub(super) use crate::bottom_pane::LocalImageAttachment;
pub(super) use crate::bottom_pane::MentionBinding;
pub(super) use crate::chatwidget::realtime::RealtimeConversationPhase;
pub(super) use crate::history_cell::UserHistoryCell;
pub(super) use crate::model_catalog::ModelCatalog;
pub(super) use crate::test_backend::VT100Backend;
pub(super) use crate::test_support::PathBufExt;
pub(super) use crate::test_support::test_path_display;
pub(super) use crate::tui::FrameRequester;
pub(super) use crate::tui_config::TuiRuntimeConfig;
pub(super) use assert_matches::assert_matches;
pub(super) use crossterm::event::KeyCode;
pub(super) use crossterm::event::KeyEvent;
pub(super) use crossterm::event::KeyModifiers;
pub(super) use insta::assert_snapshot;
pub(super) use praxis_app_gateway_protocol::AdditionalFileSystemPermissions as AppGatewayAdditionalFileSystemPermissions;
pub(super) use praxis_app_gateway_protocol::AdditionalNetworkPermissions as AppGatewayAdditionalNetworkPermissions;
pub(super) use praxis_app_gateway_protocol::AdditionalPermissionProfile as AppGatewayAdditionalPermissionProfile;
pub(super) use praxis_app_gateway_protocol::AppSummary;
pub(super) use praxis_app_gateway_protocol::CollabAgentState as AppGatewayCollabAgentState;
pub(super) use praxis_app_gateway_protocol::CollabAgentStatus as AppGatewayCollabAgentStatus;
pub(super) use praxis_app_gateway_protocol::CollabAgentTool as AppGatewayCollabAgentTool;
pub(super) use praxis_app_gateway_protocol::CollabAgentToolCallStatus as AppGatewayCollabAgentToolCallStatus;
pub(super) use praxis_app_gateway_protocol::CommandAction as AppGatewayCommandAction;
pub(super) use praxis_app_gateway_protocol::CommandExecutionRequestApprovalParams as AppGatewayCommandExecutionRequestApprovalParams;
pub(super) use praxis_app_gateway_protocol::CommandExecutionSource as AppGatewayCommandExecutionSource;
pub(super) use praxis_app_gateway_protocol::CommandExecutionStatus as AppGatewayCommandExecutionStatus;
pub(super) use praxis_app_gateway_protocol::ErrorNotification;
pub(super) use praxis_app_gateway_protocol::FileUpdateChange;
pub(super) use praxis_app_gateway_protocol::GuardianApprovalReview;
pub(super) use praxis_app_gateway_protocol::GuardianApprovalReviewAction as AppGatewayGuardianApprovalReviewAction;
pub(super) use praxis_app_gateway_protocol::GuardianApprovalReviewStatus;
pub(super) use praxis_app_gateway_protocol::GuardianCommandSource as AppGatewayGuardianCommandSource;
pub(super) use praxis_app_gateway_protocol::GuardianRiskLevel as AppGatewayGuardianRiskLevel;
pub(super) use praxis_app_gateway_protocol::HookCompletedNotification as AppGatewayHookCompletedNotification;
pub(super) use praxis_app_gateway_protocol::HookEventName as AppGatewayHookEventName;
pub(super) use praxis_app_gateway_protocol::HookExecutionMode as AppGatewayHookExecutionMode;
pub(super) use praxis_app_gateway_protocol::HookHandlerType as AppGatewayHookHandlerType;
pub(super) use praxis_app_gateway_protocol::HookOutputEntry as AppGatewayHookOutputEntry;
pub(super) use praxis_app_gateway_protocol::HookOutputEntryKind as AppGatewayHookOutputEntryKind;
pub(super) use praxis_app_gateway_protocol::HookRunStatus as AppGatewayHookRunStatus;
pub(super) use praxis_app_gateway_protocol::HookRunSummary as AppGatewayHookRunSummary;
pub(super) use praxis_app_gateway_protocol::HookScope as AppGatewayHookScope;
pub(super) use praxis_app_gateway_protocol::HookStartedNotification as AppGatewayHookStartedNotification;
pub(super) use praxis_app_gateway_protocol::ItemCompletedNotification;
pub(super) use praxis_app_gateway_protocol::ItemGuardianApprovalReviewCompletedNotification;
pub(super) use praxis_app_gateway_protocol::ItemGuardianApprovalReviewStartedNotification;
pub(super) use praxis_app_gateway_protocol::ItemStartedNotification;
pub(super) use praxis_app_gateway_protocol::MarketplaceInterface;
pub(super) use praxis_app_gateway_protocol::McpServerStartupState;
pub(super) use praxis_app_gateway_protocol::McpServerStatusUpdatedNotification;
pub(super) use praxis_app_gateway_protocol::PatchApplyStatus as AppGatewayPatchApplyStatus;
pub(super) use praxis_app_gateway_protocol::PatchChangeKind;
pub(super) use praxis_app_gateway_protocol::PermissionsRequestApprovalParams as AppGatewayPermissionsRequestApprovalParams;
pub(super) use praxis_app_gateway_protocol::PluginAuthPolicy;
pub(super) use praxis_app_gateway_protocol::PluginDetail;
pub(super) use praxis_app_gateway_protocol::PluginInstallPolicy;
pub(super) use praxis_app_gateway_protocol::PluginInterface;
pub(super) use praxis_app_gateway_protocol::PluginListResponse;
pub(super) use praxis_app_gateway_protocol::PluginMarketplaceEntry;
pub(super) use praxis_app_gateway_protocol::PluginReadResponse;
pub(super) use praxis_app_gateway_protocol::PluginSource;
pub(super) use praxis_app_gateway_protocol::PluginSummary;
pub(super) use praxis_app_gateway_protocol::ReasoningSummaryTextDeltaNotification;
pub(super) use praxis_app_gateway_protocol::ServerNotification;
pub(super) use praxis_app_gateway_protocol::SkillSummary;
pub(super) use praxis_app_gateway_protocol::ThreadClosedNotification;
pub(super) use praxis_app_gateway_protocol::ThreadItem as AppGatewayThreadItem;
pub(super) use praxis_app_gateway_protocol::Turn as AppGatewayTurn;
pub(super) use praxis_app_gateway_protocol::TurnCompletedNotification;
pub(super) use praxis_app_gateway_protocol::TurnError as AppGatewayTurnError;
pub(super) use praxis_app_gateway_protocol::TurnStartedNotification;
pub(super) use praxis_app_gateway_protocol::TurnStatus as AppGatewayTurnStatus;
pub(super) use praxis_app_gateway_protocol::UserInput as AppGatewayUserInput;
pub(super) use praxis_config::types::ApprovalsReviewer;
pub(super) use praxis_config::types::Notifications;
#[cfg(target_os = "windows")]
pub(super) use praxis_config::types::WindowsSandboxModeToml;
pub(super) use praxis_core::config::Config;
pub(super) use praxis_core::config::ConfigBuilder;
pub(super) use praxis_core::config::Constrained;
pub(super) use praxis_core::config::ConstraintError;
pub(super) use praxis_core::config_loader::AppRequirementToml;
pub(super) use praxis_core::config_loader::AppsRequirementsToml;
pub(super) use praxis_core::config_loader::ConfigLayerStack;
pub(super) use praxis_core::config_loader::ConfigRequirements;
pub(super) use praxis_core::config_loader::ConfigRequirementsToml;
pub(super) use praxis_core::config_loader::RequirementSource;
pub(super) use praxis_core::models_manager::collaboration_mode_presets::CollaborationModesConfig;
pub(super) use praxis_core::plugins::OPENAI_CURATED_MARKETPLACE_NAME;
pub(super) use praxis_core::skills::model::SkillMetadata;
pub(super) use praxis_features::FEATURES;
pub(super) use praxis_features::Feature;
pub(super) use praxis_git_utils::CommitLogEntry;
pub(super) use praxis_otel::RuntimeMetricsSummary;
pub(super) use praxis_otel::SessionTelemetry;
pub(super) use praxis_protocol::ThreadId;
pub(super) use praxis_protocol::account::PlanType;
pub(super) use praxis_protocol::config_types::CollaborationMode;
pub(super) use praxis_protocol::config_types::ModeKind;
pub(super) use praxis_protocol::config_types::Personality;
pub(super) use praxis_protocol::config_types::ServiceTier;
pub(super) use praxis_protocol::config_types::Settings;
pub(super) use praxis_protocol::items::AgentMessageContent;
pub(super) use praxis_protocol::items::AgentMessageItem;
pub(super) use praxis_protocol::items::PlanItem;
pub(super) use praxis_protocol::items::TurnItem;
pub(super) use praxis_protocol::items::UserMessageItem;
pub(super) use praxis_protocol::models::FileSystemPermissions;
pub(super) use praxis_protocol::models::MessagePhase;
pub(super) use praxis_protocol::models::NetworkPermissions;
pub(super) use praxis_protocol::models::PermissionProfile;
pub(super) use praxis_protocol::openai_models::ModelPreset;
pub(super) use praxis_protocol::openai_models::ReasoningEffortPreset;
pub(super) use praxis_protocol::openai_models::default_input_modalities;
pub(super) use praxis_protocol::parse_command::ParsedCommand;
pub(super) use praxis_protocol::plan_tool::PlanItemArg;
pub(super) use praxis_protocol::plan_tool::StepStatus;
pub(super) use praxis_protocol::plan_tool::UpdatePlanArgs;
pub(super) use praxis_protocol::protocol::AgentMessageDeltaEvent;
pub(super) use praxis_protocol::protocol::AgentMessageEvent;
pub(super) use praxis_protocol::protocol::AgentReasoningDeltaEvent;
pub(super) use praxis_protocol::protocol::AgentReasoningEvent;
pub(super) use praxis_protocol::protocol::AgentStatus;
pub(super) use praxis_protocol::protocol::ApplyPatchApprovalRequestEvent;
pub(super) use praxis_protocol::protocol::BackgroundEventEvent;
pub(super) use praxis_protocol::protocol::CodexErrorInfo;
pub(super) use praxis_protocol::protocol::CollabAgentSpawnBeginEvent;
pub(super) use praxis_protocol::protocol::CollabAgentSpawnEndEvent;
pub(super) use praxis_protocol::protocol::CreditsSnapshot;
pub(super) use praxis_protocol::protocol::Event;
pub(super) use praxis_protocol::protocol::EventMsg;
pub(super) use praxis_protocol::protocol::ExecApprovalRequestEvent;
pub(super) use praxis_protocol::protocol::ExecCommandBeginEvent;
pub(super) use praxis_protocol::protocol::ExecCommandEndEvent;
pub(super) use praxis_protocol::protocol::ExecCommandSource;
pub(super) use praxis_protocol::protocol::ExecCommandStatus as CoreExecCommandStatus;
pub(super) use praxis_protocol::protocol::ExecPolicyAmendment;
pub(super) use praxis_protocol::protocol::ExitedReviewModeEvent;
pub(super) use praxis_protocol::protocol::FileChange;
pub(super) use praxis_protocol::protocol::GuardianAssessmentAction;
pub(super) use praxis_protocol::protocol::GuardianAssessmentEvent;
pub(super) use praxis_protocol::protocol::GuardianAssessmentStatus;
pub(super) use praxis_protocol::protocol::GuardianCommandSource;
pub(super) use praxis_protocol::protocol::GuardianRiskLevel;
pub(super) use praxis_protocol::protocol::ImageGenerationEndEvent;
pub(super) use praxis_protocol::protocol::ItemCompletedEvent;
pub(super) use praxis_protocol::protocol::McpStartupCompleteEvent;
pub(super) use praxis_protocol::protocol::McpStartupStatus;
pub(super) use praxis_protocol::protocol::McpStartupUpdateEvent;
pub(super) use praxis_protocol::protocol::NonSteerableTurnKind;
pub(super) use praxis_protocol::protocol::Op;
pub(super) use praxis_protocol::protocol::PatchApplyBeginEvent;
pub(super) use praxis_protocol::protocol::PatchApplyEndEvent;
pub(super) use praxis_protocol::protocol::PatchApplyStatus as CorePatchApplyStatus;
pub(super) use praxis_protocol::protocol::RateLimitWindow;
pub(super) use praxis_protocol::protocol::ReadOnlyAccess;
pub(super) use praxis_protocol::protocol::RealtimeConversationClosedEvent;
pub(super) use praxis_protocol::protocol::RealtimeConversationRealtimeEvent;
pub(super) use praxis_protocol::protocol::RealtimeEvent;
pub(super) use praxis_protocol::protocol::ReviewRequest;
pub(super) use praxis_protocol::protocol::ReviewTarget;
pub(super) use praxis_protocol::protocol::SessionConfiguredEvent;
pub(super) use praxis_protocol::protocol::SessionSource;
pub(super) use praxis_protocol::protocol::SkillScope;
pub(super) use praxis_protocol::protocol::StreamErrorEvent;
pub(super) use praxis_protocol::protocol::TerminalInteractionEvent;
pub(super) use praxis_protocol::protocol::ThreadRolledBackEvent;
pub(super) use praxis_protocol::protocol::TokenCountEvent;
pub(super) use praxis_protocol::protocol::TokenUsage;
pub(super) use praxis_protocol::protocol::TokenUsageInfo;
pub(super) use praxis_protocol::protocol::TurnCompleteEvent;
pub(super) use praxis_protocol::protocol::TurnStartedEvent;
pub(super) use praxis_protocol::protocol::UndoCompletedEvent;
pub(super) use praxis_protocol::protocol::UndoStartedEvent;
pub(super) use praxis_protocol::protocol::ViewImageToolCallEvent;
pub(super) use praxis_protocol::protocol::WarningEvent;
pub(super) use praxis_protocol::request_permissions::RequestPermissionProfile;
pub(super) use praxis_protocol::request_user_input::RequestUserInputEvent;
pub(super) use praxis_protocol::request_user_input::RequestUserInputQuestion;
pub(super) use praxis_protocol::request_user_input::RequestUserInputQuestionOption;
pub(super) use praxis_protocol::user_input::TextElement;
pub(super) use praxis_protocol::user_input::UserInput;
pub(super) use praxis_terminal_detection::Multiplexer;
pub(super) use praxis_terminal_detection::TerminalInfo;
pub(super) use praxis_terminal_detection::TerminalName;
pub(super) use praxis_utils_absolute_path::AbsolutePathBuf;
pub(super) use praxis_utils_approval_presets::builtin_approval_presets;
#[cfg(target_os = "windows")]
pub(super) use serial_test::serial;
pub(super) use std::collections::BTreeMap;
pub(super) use std::collections::HashMap;
pub(super) use std::collections::HashSet;
pub(super) use std::path::PathBuf;
pub(super) use tempfile::NamedTempFile;
pub(super) use tempfile::tempdir;
pub(super) use tokio::sync::mpsc::error::TryRecvError;
pub(super) use tokio::sync::mpsc::unbounded_channel;
pub(super) use toml::Value as TomlValue;

pub(super) fn chatwidget_snapshot_dir() -> PathBuf {
    praxis_utils_cargo_bin::find_resource!("src/chatwidget/snapshots").expect("snapshot dir")
}

macro_rules! assert_chatwidget_snapshot {
    ($name:expr, $value:expr $(,)?) => {{
        let mut settings = insta::Settings::clone_current();
        settings.set_prepend_module_to_snapshot(false);
        settings.set_snapshot_path(crate::chatwidget::tests::chatwidget_snapshot_dir());
        settings.bind(|| {
            insta::assert_snapshot!(format!("praxis_tui__chatwidget__tests__{}", $name), $value);
        });
    }};
    ($name:expr, $value:expr, @$snapshot:literal $(,)?) => {{
        let mut settings = insta::Settings::clone_current();
        settings.set_prepend_module_to_snapshot(false);
        settings.set_snapshot_path(crate::chatwidget::tests::chatwidget_snapshot_dir());
        settings.bind(|| {
            insta::assert_snapshot!(
                format!("praxis_tui__chatwidget__tests__{}", $name),
                &($value),
                @$snapshot
            );
        });
    }};
}

mod app_gateway;
mod approval_requests;
mod background_events;
mod composer_submission;
mod exec_flow;
mod guardian;
mod helpers;
mod history_replay;
mod mcp_startup;
mod permissions;
mod plan_mode;
mod popups_and_settings;
mod review_mode;
mod slash_commands;
mod status_and_layout;
mod status_command_tests;
mod transcript_search;

pub(crate) use helpers::make_chatwidget_manual_with_sender;
pub(crate) use helpers::set_chatgpt_auth;
pub(super) use helpers::*;
