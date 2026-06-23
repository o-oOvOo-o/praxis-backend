use super::app_gateway_fetch::build_feedback_upload_params;
use super::*;
use crate::app_backtrack::BacktrackSelection;
use crate::app_backtrack::BacktrackState;
use crate::app_backtrack::user_count;

use crate::chatwidget::ChatWidgetInit;
use crate::chatwidget::create_initial_user_message;
use crate::chatwidget::tests::make_chatwidget_manual_with_sender;
use crate::chatwidget::tests::set_chatgpt_auth;
use crate::file_search::FileSearchManager;
use crate::history_cell::AgentMessageCell;
use crate::history_cell::HistoryCell;
use crate::history_cell::UserHistoryCell;
use crate::history_cell::new_session_info;
use crate::multi_agents::AgentPickerThreadEntry;
use assert_matches::assert_matches;

use crossterm::event::KeyModifiers;
use insta::assert_snapshot;
use praxis_app_gateway_protocol::AdditionalFileSystemPermissions;
use praxis_app_gateway_protocol::AdditionalNetworkPermissions;
use praxis_app_gateway_protocol::AdditionalPermissionProfile;
use praxis_app_gateway_protocol::AgentMessageDeltaNotification;
use praxis_app_gateway_protocol::CommandExecutionRequestApprovalParams;
use praxis_app_gateway_protocol::ConfigWarningNotification;
use praxis_app_gateway_protocol::HookCompletedNotification;
use praxis_app_gateway_protocol::HookEventName as AppGatewayHookEventName;
use praxis_app_gateway_protocol::HookExecutionMode as AppGatewayHookExecutionMode;
use praxis_app_gateway_protocol::HookHandlerType as AppGatewayHookHandlerType;
use praxis_app_gateway_protocol::HookOutputEntry as AppGatewayHookOutputEntry;
use praxis_app_gateway_protocol::HookOutputEntryKind as AppGatewayHookOutputEntryKind;
use praxis_app_gateway_protocol::HookRunStatus as AppGatewayHookRunStatus;
use praxis_app_gateway_protocol::HookRunSummary as AppGatewayHookRunSummary;
use praxis_app_gateway_protocol::HookScope as AppGatewayHookScope;
use praxis_app_gateway_protocol::HookStartedNotification;
use praxis_app_gateway_protocol::JSONRPCErrorError;
use praxis_app_gateway_protocol::McpServerStatus;
use praxis_app_gateway_protocol::NetworkApprovalContext as AppGatewayNetworkApprovalContext;
use praxis_app_gateway_protocol::NetworkApprovalProtocol as AppGatewayNetworkApprovalProtocol;
use praxis_app_gateway_protocol::NetworkPolicyAmendment as AppGatewayNetworkPolicyAmendment;
use praxis_app_gateway_protocol::NetworkPolicyRuleAction as AppGatewayNetworkPolicyRuleAction;
use praxis_app_gateway_protocol::NonSteerableTurnKind as AppGatewayNonSteerableTurnKind;
use praxis_app_gateway_protocol::PermissionsRequestApprovalParams;
use praxis_app_gateway_protocol::RequestId as AppGatewayRequestId;
use praxis_app_gateway_protocol::ServerNotification;
use praxis_app_gateway_protocol::ServerRequest;
use praxis_app_gateway_protocol::Thread;
use praxis_app_gateway_protocol::ThreadClosedNotification;
use praxis_app_gateway_protocol::ThreadItem;
use praxis_app_gateway_protocol::ThreadStartedNotification;
use praxis_app_gateway_protocol::ThreadTokenUsage;
use praxis_app_gateway_protocol::ThreadTokenUsageUpdatedNotification;
use praxis_app_gateway_protocol::TokenUsageBreakdown;
use praxis_app_gateway_protocol::Turn;
use praxis_app_gateway_protocol::TurnCompletedNotification;
use praxis_app_gateway_protocol::TurnError as AppGatewayTurnError;
use praxis_app_gateway_protocol::TurnStartedNotification;
use praxis_app_gateway_protocol::TurnStatus;
use praxis_app_gateway_protocol::UserInput as AppGatewayUserInput;
use praxis_config::types::ModelAvailabilityNuxConfig;
use praxis_core::config::ConfigBuilder;
use praxis_core::config::ConfigOverrides;
use praxis_otel::SessionTelemetry;
use praxis_protocol::ThreadId;
use praxis_protocol::config_types::CollaborationMode;
use praxis_protocol::config_types::CollaborationModeMask;
use praxis_protocol::config_types::ModeKind;
use praxis_protocol::config_types::Settings;
use praxis_protocol::mcp::Tool;
use praxis_protocol::models::FileSystemPermissions;
use praxis_protocol::models::NetworkPermissions;
use praxis_protocol::models::PermissionProfile;
use praxis_protocol::openai_models::ModelAvailabilityNux;
use praxis_protocol::protocol::AskForApproval;
use praxis_protocol::protocol::Event;
use praxis_protocol::protocol::EventMsg;
use praxis_protocol::protocol::McpAuthStatus;
use praxis_protocol::protocol::NetworkApprovalContext;
use praxis_protocol::protocol::NetworkApprovalProtocol;
use praxis_protocol::protocol::RolloutItem;
use praxis_protocol::protocol::RolloutLine;
use praxis_protocol::protocol::SandboxPolicy;
use praxis_protocol::protocol::SessionConfiguredEvent;
use praxis_protocol::protocol::SessionSource;
use praxis_protocol::protocol::TurnContextItem;
use praxis_protocol::request_permissions::RequestPermissionProfile;
use praxis_protocol::user_input::TextElement;
use praxis_protocol::user_input::UserInput;
use praxis_utils_absolute_path::AbsolutePathBuf;
use pretty_assertions::assert_eq;
use ratatui::prelude::Line;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use tempfile::tempdir;
use tokio::time;

fn test_absolute_path(path: &str) -> AbsolutePathBuf {
    AbsolutePathBuf::try_from(PathBuf::from(path)).expect("absolute test path")
}

type McpInventoryMaps = (
    HashMap<String, Tool>,
    HashMap<String, Vec<praxis_protocol::mcp::Resource>>,
    HashMap<String, Vec<praxis_protocol::mcp::ResourceTemplate>>,
    HashMap<String, McpAuthStatus>,
);

fn mcp_inventory_maps_from_statuses(statuses: Vec<McpServerStatus>) -> McpInventoryMaps {
    let mut tools = HashMap::new();
    let mut resources = HashMap::new();
    let mut resource_templates = HashMap::new();
    let mut auth_statuses = HashMap::new();

    for status in statuses {
        let server_name = status.name;
        auth_statuses.insert(
            server_name.clone(),
            match status.auth_status {
                praxis_app_gateway_protocol::McpAuthStatus::Unsupported => {
                    McpAuthStatus::Unsupported
                }
                praxis_app_gateway_protocol::McpAuthStatus::NotLoggedIn => {
                    McpAuthStatus::NotLoggedIn
                }
                praxis_app_gateway_protocol::McpAuthStatus::BearerToken => {
                    McpAuthStatus::BearerToken
                }
                praxis_app_gateway_protocol::McpAuthStatus::OAuth => McpAuthStatus::OAuth,
            },
        );
        resources.insert(server_name.clone(), status.resources);
        resource_templates.insert(server_name.clone(), status.resource_templates);
        for (tool_name, tool) in status.tools {
            tools.insert(format!("mcp__{server_name}__{tool_name}"), tool);
        }
    }

    (tools, resources, resource_templates, auth_statuses)
}

#[path = "app_tests/agent_picker.rs"]
mod agent_picker;
#[path = "app_tests/backtrack_and_shutdown.rs"]
mod backtrack_and_shutdown;
#[path = "app_tests/clear_ui.rs"]
mod clear_ui;
#[path = "app_tests/guardian_policy.rs"]
mod guardian_policy;
#[path = "app_tests/inactive_thread_approvals.rs"]
mod inactive_thread_approvals;
#[path = "app_tests/model_and_config.rs"]
mod model_and_config;
#[path = "app_tests/startup_and_replay.rs"]
mod startup_and_replay;
#[path = "app_tests/thread_event_store.rs"]
mod thread_event_store;

async fn make_test_app() -> App {
    let (chat_widget, app_event_tx, _rx, _op_rx) = make_chatwidget_manual_with_sender().await;
    let config = chat_widget.config_ref().clone();
    let tui_config = chat_widget.tui_config_ref().clone();
    let file_search = FileSearchManager::new(config.cwd.to_path_buf(), app_event_tx.clone());
    let model = praxis_core::test_support::get_model_offline(config.model.as_deref());
    let session_telemetry = test_session_telemetry(&config, model.as_str());

    App {
        model_catalog: chat_widget.model_catalog(),
        session_telemetry,
        app_event_tx,
        chat_widget,
        config,
        tui_config,
        active_profile: None,
        cli_kv_overrides: Vec::new(),
        harness_overrides: ConfigOverrides::default(),
        runtime_approval_policy_override: None,
        runtime_sandbox_policy_override: None,
        file_search,
        transcript_cells: Vec::new(),
        overlay: None,
        deferred_history_lines: Vec::new(),
        has_emitted_history_lines: false,
        enhanced_keys_supported: false,
        commit_anim_running: Arc::new(AtomicBool::new(false)),
        status_line_invalid_items_warned: Arc::new(AtomicBool::new(false)),
        terminal_title_invalid_items_warned: Arc::new(AtomicBool::new(false)),
        backtrack: BacktrackState::default(),
        backtrack_render_pending: false,
        transcript_scrollback_backfill: None,
        feedback: praxis_feedback::PraxisFeedback::new(),
        feedback_audience: FeedbackAudience::External,
        remote_app_gateway_url: None,
        remote_app_gateway_auth_token: None,
        app_gateway_reconnect_pending: false,
        last_app_gateway_reconnect_attempt: None,
        pending_update_action: None,
        pending_shutdown_exit_thread_id: None,
        windows_sandbox: WindowsSandboxState::default(),
        thread_event_channels: HashMap::new(),
        thread_event_listener_tasks: HashMap::new(),
        agent_navigation: AgentNavigationState::default(),
        active_thread_id: None,
        active_thread_rx: None,
        primary_thread_id: None,
        last_subagent_backfill_attempt: None,
        primary_session_configured: None,
        pending_primary_events: VecDeque::new(),
        pending_app_gateway_requests: PendingAppGatewayRequests::default(),
        workspace: WorkspaceState::new(false),
        workspace_observed_thread_ids: HashSet::new(),
        mouse: MouseInteractionState::default(),
        mouse_capture_resume_at: None,
    }
}

async fn make_test_app_with_channels() -> (
    App,
    tokio::sync::mpsc::UnboundedReceiver<AppEvent>,
    tokio::sync::mpsc::UnboundedReceiver<Op>,
) {
    let (chat_widget, app_event_tx, rx, op_rx) = make_chatwidget_manual_with_sender().await;
    let config = chat_widget.config_ref().clone();
    let tui_config = chat_widget.tui_config_ref().clone();
    let file_search = FileSearchManager::new(config.cwd.to_path_buf(), app_event_tx.clone());
    let model = praxis_core::test_support::get_model_offline(config.model.as_deref());
    let session_telemetry = test_session_telemetry(&config, model.as_str());

    (
        App {
            model_catalog: chat_widget.model_catalog(),
            session_telemetry,
            app_event_tx,
            chat_widget,
            config,
            tui_config,
            active_profile: None,
            cli_kv_overrides: Vec::new(),
            harness_overrides: ConfigOverrides::default(),
            runtime_approval_policy_override: None,
            runtime_sandbox_policy_override: None,
            file_search,
            transcript_cells: Vec::new(),
            overlay: None,
            deferred_history_lines: Vec::new(),
            has_emitted_history_lines: false,
            enhanced_keys_supported: false,
            commit_anim_running: Arc::new(AtomicBool::new(false)),
            status_line_invalid_items_warned: Arc::new(AtomicBool::new(false)),
            terminal_title_invalid_items_warned: Arc::new(AtomicBool::new(false)),
            backtrack: BacktrackState::default(),
            backtrack_render_pending: false,
            transcript_scrollback_backfill: None,
            feedback: praxis_feedback::PraxisFeedback::new(),
            feedback_audience: FeedbackAudience::External,
            remote_app_gateway_url: None,
            remote_app_gateway_auth_token: None,
            app_gateway_reconnect_pending: false,
            last_app_gateway_reconnect_attempt: None,
            pending_update_action: None,
            pending_shutdown_exit_thread_id: None,
            windows_sandbox: WindowsSandboxState::default(),
            thread_event_channels: HashMap::new(),
            thread_event_listener_tasks: HashMap::new(),
            agent_navigation: AgentNavigationState::default(),
            active_thread_id: None,
            active_thread_rx: None,
            primary_thread_id: None,
            last_subagent_backfill_attempt: None,
            primary_session_configured: None,
            pending_primary_events: VecDeque::new(),
            pending_app_gateway_requests: PendingAppGatewayRequests::default(),
            workspace: WorkspaceState::new(false),
            workspace_observed_thread_ids: HashSet::new(),
            mouse: MouseInteractionState::default(),
            mouse_capture_resume_at: None,
        },
        rx,
        op_rx,
    )
}

fn test_thread_session(thread_id: ThreadId, cwd: PathBuf) -> ThreadSessionState {
    ThreadSessionState {
        thread_id,
        forked_from_id: None,
        thread_name: None,
        model: "gpt-test".to_string(),
        model_provider_id: "test-provider".to_string(),
        service_tier: None,
        approval_policy: AskForApproval::Never,
        approvals_reviewer: ApprovalsReviewer::User,
        sandbox_policy: SandboxPolicy::new_read_only_policy(),
        cwd,
        reasoning_effort: None,
        history_log_id: 0,
        history_entry_count: 0,
        network_proxy: None,
        rollout_path: Some(PathBuf::new()),
        selfwork_plan_path: None,
    }
}

fn test_turn(turn_id: &str, status: TurnStatus, items: Vec<ThreadItem>) -> Turn {
    Turn {
        id: turn_id.to_string(),
        items,
        status,
        error: None,
    }
}

fn turn_started_notification(thread_id: ThreadId, turn_id: &str) -> ServerNotification {
    ServerNotification::TurnStarted(TurnStartedNotification {
        thread_id: thread_id.to_string(),
        turn: test_turn(turn_id, TurnStatus::InProgress, Vec::new()),
        model_context_window: None,
    })
}

fn turn_completed_notification(
    thread_id: ThreadId,
    turn_id: &str,
    status: TurnStatus,
) -> ServerNotification {
    ServerNotification::TurnCompleted(TurnCompletedNotification {
        thread_id: thread_id.to_string(),
        turn: test_turn(turn_id, status, Vec::new()),
    })
}

fn thread_closed_notification(thread_id: ThreadId) -> ServerNotification {
    ServerNotification::ThreadClosed(ThreadClosedNotification {
        thread_id: thread_id.to_string(),
    })
}

fn token_usage_notification(
    thread_id: ThreadId,
    turn_id: &str,
    model_context_window: Option<i64>,
) -> ServerNotification {
    ServerNotification::ThreadTokenUsageUpdated(ThreadTokenUsageUpdatedNotification {
        thread_id: thread_id.to_string(),
        turn_id: turn_id.to_string(),
        token_usage: ThreadTokenUsage {
            total: TokenUsageBreakdown {
                total_tokens: 10,
                input_tokens: 4,
                cached_input_tokens: 1,
                cache_reported_input_tokens: 4,
                output_tokens: 5,
                reasoning_output_tokens: 0,
            },
            last: TokenUsageBreakdown {
                total_tokens: 10,
                input_tokens: 4,
                cached_input_tokens: 1,
                cache_reported_input_tokens: 4,
                output_tokens: 5,
                reasoning_output_tokens: 0,
            },
            model_context_window,
            model_auto_compact_token_limit: None,
        },
    })
}

fn hook_started_notification(thread_id: ThreadId, turn_id: &str) -> ServerNotification {
    ServerNotification::HookStarted(HookStartedNotification {
        thread_id: thread_id.to_string(),
        turn_id: Some(turn_id.to_string()),
        run: AppGatewayHookRunSummary {
            id: "user-prompt-submit:0:/tmp/hooks.json".to_string(),
            event_name: AppGatewayHookEventName::UserPromptSubmit,
            handler_type: AppGatewayHookHandlerType::Command,
            execution_mode: AppGatewayHookExecutionMode::Sync,
            scope: AppGatewayHookScope::Turn,
            source_path: PathBuf::from("/tmp/hooks.json"),
            display_order: 0,
            status: AppGatewayHookRunStatus::Running,
            status_message: Some("checking go-workflow input policy".to_string()),
            started_at: 1,
            completed_at: None,
            duration_ms: None,
            entries: Vec::new(),
        },
    })
}

fn hook_completed_notification(thread_id: ThreadId, turn_id: &str) -> ServerNotification {
    ServerNotification::HookCompleted(HookCompletedNotification {
        thread_id: thread_id.to_string(),
        turn_id: Some(turn_id.to_string()),
        run: AppGatewayHookRunSummary {
            id: "user-prompt-submit:0:/tmp/hooks.json".to_string(),
            event_name: AppGatewayHookEventName::UserPromptSubmit,
            handler_type: AppGatewayHookHandlerType::Command,
            execution_mode: AppGatewayHookExecutionMode::Sync,
            scope: AppGatewayHookScope::Turn,
            source_path: PathBuf::from("/tmp/hooks.json"),
            display_order: 0,
            status: AppGatewayHookRunStatus::Stopped,
            status_message: Some("checking go-workflow input policy".to_string()),
            started_at: 1,
            completed_at: Some(11),
            duration_ms: Some(10),
            entries: vec![
                AppGatewayHookOutputEntry {
                    kind: AppGatewayHookOutputEntryKind::Warning,
                    text: "go-workflow must start from PlanMode".to_string(),
                },
                AppGatewayHookOutputEntry {
                    kind: AppGatewayHookOutputEntryKind::Stop,
                    text: "prompt blocked".to_string(),
                },
            ],
        },
    })
}

fn agent_message_delta_notification(
    thread_id: ThreadId,
    turn_id: &str,
    item_id: &str,
    delta: &str,
) -> ServerNotification {
    ServerNotification::AgentMessageDelta(AgentMessageDeltaNotification {
        thread_id: thread_id.to_string(),
        turn_id: turn_id.to_string(),
        item_id: item_id.to_string(),
        delta: delta.to_string(),
    })
}

fn exec_approval_request(
    thread_id: ThreadId,
    turn_id: &str,
    item_id: &str,
    approval_id: Option<&str>,
) -> ServerRequest {
    ServerRequest::CommandExecutionRequestApproval {
        request_id: AppGatewayRequestId::Integer(1),
        params: CommandExecutionRequestApprovalParams {
            thread_id: thread_id.to_string(),
            turn_id: turn_id.to_string(),
            item_id: item_id.to_string(),
            approval_id: approval_id.map(str::to_string),
            reason: Some("needs approval".to_string()),
            network_approval_context: None,
            command: Some("echo hello".to_string()),
            cwd: Some(PathBuf::from("/tmp/project")),
            command_actions: None,
            additional_permissions: None,
            proposed_execpolicy_amendment: None,
            proposed_network_policy_amendments: None,
            available_decisions: None,
        },
    }
}

fn next_user_turn_op(op_rx: &mut tokio::sync::mpsc::UnboundedReceiver<Op>) -> Op {
    let mut seen = Vec::new();
    while let Ok(op) = op_rx.try_recv() {
        if matches!(op, Op::UserTurn { .. }) {
            return op;
        }
        seen.push(format!("{op:?}"));
    }
    panic!("expected UserTurn op, saw: {seen:?}");
}

fn lines_to_single_string(lines: &[Line<'_>]) -> String {
    lines
        .iter()
        .map(|line| {
            line.spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn test_session_telemetry(config: &Config, model: &str) -> SessionTelemetry {
    let model_info = praxis_core::test_support::construct_model_info_offline(model, config);
    SessionTelemetry::new(
        ThreadId::new(),
        model,
        model_info.slug.as_str(),
        /*account_id*/ None,
        /*account_email*/ None,
        /*auth_mode*/ None,
        "test_originator".to_string(),
        /*log_user_prompts*/ false,
        "test".to_string(),
        SessionSource::Cli,
    )
}

fn app_enabled_in_effective_config(config: &Config, app_id: &str) -> Option<bool> {
    config
        .config_layer_stack
        .effective_config()
        .as_table()
        .and_then(|table| table.get("apps"))
        .and_then(TomlValue::as_table)
        .and_then(|apps| apps.get(app_id))
        .and_then(TomlValue::as_table)
        .and_then(|app| app.get("enabled"))
        .and_then(TomlValue::as_bool)
}

fn all_model_presets() -> Vec<ModelPreset> {
    praxis_core::test_support::all_model_presets().clone()
}

fn model_availability_nux_config(shown_count: &[(&str, u32)]) -> ModelAvailabilityNuxConfig {
    ModelAvailabilityNuxConfig {
        shown_count: shown_count
            .iter()
            .map(|(model, count)| ((*model).to_string(), *count))
            .collect(),
    }
}

fn model_migration_copy_to_plain_text(copy: &crate::model_migration::ModelMigrationCopy) -> String {
    if let Some(markdown) = copy.markdown.as_ref() {
        return markdown.clone();
    }
    let mut s = String::new();
    for span in &copy.heading {
        s.push_str(&span.content);
    }
    s.push('\n');
    s.push('\n');
    for line in &copy.content {
        for span in &line.spans {
            s.push_str(&span.content);
        }
        s.push('\n');
    }
    s
}
