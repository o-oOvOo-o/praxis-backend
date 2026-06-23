use super::model_request::collect_explicit_app_ids_from_skill_items;
use super::model_request::filter_connectors_for_input;
use super::model_request::filter_praxis_apps_mcp_tools;
use super::*;
use crate::config::ConfigBuilder;
use crate::config::test_config;
use crate::config_loader::ConfigLayerStack;
use crate::config_loader::ConfigLayerStackOrdering;
use crate::config_loader::NetworkConstraints;
use crate::config_loader::NetworkDomainPermissionToml;
use crate::config_loader::NetworkDomainPermissionsToml;
use crate::config_loader::RequirementSource;
use crate::config_loader::Sourced;
use crate::exec::ExecCapturePolicy;
use crate::exec::ExecToolCallOutput;
use crate::function_tool::FunctionCallError;
use crate::models_manager::model_info;
use crate::shell::default_user_shell;
use crate::tools::format_exec_output_str;

use praxis_features::Features;
use praxis_login::OpenAiAccountAuth;
use praxis_mcp::mcp_connection_manager::ToolInfo;
use praxis_protocol::ThreadId;
use praxis_protocol::models::FunctionCallOutputBody;
use praxis_protocol::models::FunctionCallOutputPayload;
use praxis_protocol::permissions::FileSystemAccessMode;
use praxis_protocol::permissions::FileSystemPath;
use praxis_protocol::permissions::FileSystemSandboxEntry;
use praxis_protocol::permissions::FileSystemSandboxPolicy;
use praxis_protocol::permissions::FileSystemSpecialPath;
use praxis_protocol::protocol::NonSteerableTurnKind;
use praxis_protocol::protocol::ReadOnlyAccess;
use praxis_protocol::protocol::SandboxPolicy;
use praxis_protocol::request_permissions::PermissionGrantScope;
use praxis_protocol::request_permissions::RequestPermissionProfile;
use tracing::Span;

use crate::rollout::policy::EventPersistenceMode;
use crate::rollout::recorder::RolloutRecorder;
use crate::rollout::recorder::RolloutRecorderParams;
use crate::state::AgentTaskKind;
use crate::tasks::AgentTask;
use crate::tasks::AgentTaskContext;
use crate::tools::ToolRouter;
use crate::tools::context::FunctionToolOutput;
use crate::tools::context::ToolInvocation;
use crate::tools::context::ToolPayload;
use crate::tools::handlers::ShellHandler;
use crate::tools::handlers::UnifiedExecHandler;
use crate::tools::registry::ToolHandler;
use crate::tools::router::ToolCallSource;
use crate::turn_completed_output::CompletedOutputCtx;
use crate::turn_completed_output::handle_completed_output_item;
use crate::turn_diff_tracker::TurnDiffTracker;
use core_test_support::PathBufExt;
use core_test_support::context_snapshot;
use core_test_support::context_snapshot::ContextSnapshotOptions;
use core_test_support::context_snapshot::ContextSnapshotRenderMode;
use core_test_support::responses::ev_completed;
use core_test_support::responses::ev_response_created;
use core_test_support::responses::mount_sse_once;
use core_test_support::responses::sse;
use core_test_support::responses::start_mock_server;
use core_test_support::test_praxis::test_praxis;
use core_test_support::tracing::install_test_tracing;
use core_test_support::wait_for_event;
use opentelemetry::trace::TraceContextExt;
use opentelemetry::trace::TraceId;
use praxis_execpolicy::Decision;
use praxis_execpolicy::NetworkRuleProtocol;
use praxis_execpolicy::Policy;
use praxis_network_proxy::NetworkProxyConfig;
use praxis_otel::TelemetryAuthMode;
use praxis_protocol::apps::AppInfo;
use praxis_protocol::config_types::CollaborationMode;
use praxis_protocol::config_types::ModeKind;
use praxis_protocol::config_types::Settings;
use praxis_protocol::models::BaseInstructions;
use praxis_protocol::models::ContentItem;
use praxis_protocol::models::DeveloperInstructions;
use praxis_protocol::models::ResponseInputItem;
use praxis_protocol::models::ResponseItem;
use praxis_protocol::openai_models::ModelsResponse;
use praxis_protocol::protocol::AskForApproval;
use praxis_protocol::protocol::CompactedItem;
use praxis_protocol::protocol::ConversationAudioParams;
use praxis_protocol::protocol::CreditsSnapshot;
use praxis_protocol::protocol::GranularApprovalConfig;
use praxis_protocol::protocol::InitialHistory;
use praxis_protocol::protocol::NetworkApprovalProtocol;
use praxis_protocol::protocol::RateLimitSnapshot;
use praxis_protocol::protocol::RateLimitWindow;
use praxis_protocol::protocol::RealtimeAudioFrame;
use praxis_protocol::protocol::ResumedHistory;
use praxis_protocol::protocol::RolloutItem;
use praxis_protocol::protocol::Submission;
use praxis_protocol::protocol::ThreadRolledBackEvent;
use praxis_protocol::protocol::TokenCountEvent;
use praxis_protocol::protocol::TokenUsage;
use praxis_protocol::protocol::TokenUsageInfo;
use praxis_protocol::protocol::TurnAbortedEvent;
use praxis_protocol::protocol::TurnCompleteEvent;
use praxis_protocol::protocol::TurnStartedEvent;
use praxis_protocol::protocol::UserMessageEvent;
use praxis_protocol::protocol::W3cTraceContext;
use std::path::Path;
use std::time::Duration;
use tokio::time::sleep;
use tracing_opentelemetry::OpenTelemetrySpanExt;

use praxis_protocol::mcp::CallToolResult as McpCallToolResult;
use pretty_assertions::assert_eq;
use rmcp::model::JsonObject;
use rmcp::model::Tool;
use serde::Deserialize;
use serde_json::json;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration as StdDuration;

#[path = "praxis_tests_guardian.rs"]
mod guardian_tests;

#[path = "praxis_tests/exec_permission_rejection.rs"]
mod exec_permission_rejection;
#[path = "praxis_tests/permissions_and_tracing.rs"]
mod permissions_and_tracing;
#[path = "praxis_tests/session_config.rs"]
mod session_config;
#[path = "praxis_tests/session_history.rs"]
mod session_history;
#[path = "praxis_tests/shutdown_and_tasks.rs"]
mod shutdown_and_tasks;

use praxis_protocol::models::function_call_output_content_items_to_text;

fn expect_text_tool_output(output: &FunctionToolOutput) -> String {
    function_call_output_content_items_to_text(&output.body).unwrap_or_default()
}

struct InstructionsTestCase {
    slug: &'static str,
    expects_apply_patch_instructions: bool,
}

fn user_message(text: &str) -> ResponseItem {
    ResponseItem::Message {
        id: None,
        role: "user".to_string(),
        content: vec![ContentItem::InputText {
            text: text.to_string(),
        }],
        end_turn: None,
        phase: None,
    }
}

fn assistant_message(text: &str) -> ResponseItem {
    ResponseItem::Message {
        id: None,
        role: "assistant".to_string(),
        content: vec![ContentItem::OutputText {
            text: text.to_string(),
        }],
        end_turn: None,
        phase: None,
    }
}

fn skill_message(text: &str) -> ResponseItem {
    ResponseItem::Message {
        id: None,
        role: "user".to_string(),
        content: vec![ContentItem::InputText {
            text: text.to_string(),
        }],
        end_turn: None,
        phase: None,
    }
}

async fn wait_for_thread_rolled_back(rx: &async_channel::Receiver<Event>) -> ThreadRolledBackEvent {
    let deadline = StdDuration::from_secs(2);
    let start = std::time::Instant::now();
    loop {
        let remaining = deadline.saturating_sub(start.elapsed());
        let evt = tokio::time::timeout(remaining, rx.recv())
            .await
            .expect("timeout waiting for event")
            .expect("event");
        match evt.msg {
            EventMsg::ThreadRolledBack(payload) => return payload,
            _ => continue,
        }
    }
}

async fn wait_for_thread_rollback_failed(rx: &async_channel::Receiver<Event>) -> ErrorEvent {
    let deadline = StdDuration::from_secs(2);
    let start = std::time::Instant::now();
    loop {
        let remaining = deadline.saturating_sub(start.elapsed());
        let evt = tokio::time::timeout(remaining, rx.recv())
            .await
            .expect("timeout waiting for event")
            .expect("event");
        match evt.msg {
            EventMsg::Error(payload)
                if payload.praxis_error_info == Some(PraxisErrorInfo::ThreadRollbackFailed) =>
            {
                return payload;
            }
            _ => continue,
        }
    }
}

async fn attach_rollout_recorder(session: &Arc<Session>) -> PathBuf {
    let config = session.get_config().await;
    let recorder = RolloutRecorder::new(
        config.as_ref(),
        RolloutRecorderParams::new(
            ThreadId::default(),
            /*forked_from_id*/ None,
            SessionSource::Exec,
            BaseInstructions::default(),
            Vec::new(),
            EventPersistenceMode::Limited,
        ),
        /*state_db_ctx*/ None,
        /*state_builder*/ None,
    )
    .await
    .expect("create rollout recorder");
    let rollout_path = recorder.rollout_path().to_path_buf();
    {
        let mut rollout = session.services.rollout.lock().await;
        *rollout = Some(recorder);
    }
    session.ensure_rollout_materialized().await;
    session.flush_rollout().await;
    rollout_path
}

fn text_block(s: &str) -> serde_json::Value {
    json!({
        "type": "text",
        "text": s,
    })
}

async fn build_test_config(praxis_home: &Path) -> Config {
    ConfigBuilder::default()
        .praxis_home(praxis_home.to_path_buf())
        .build()
        .await
        .expect("load default test config")
}

fn session_telemetry(
    conversation_id: ThreadId,
    config: &Config,
    model_info: &ModelInfo,
    session_source: SessionSource,
) -> SessionTelemetry {
    SessionTelemetry::new(
        conversation_id,
        ModelsManager::get_model_offline_for_tests(config.model.as_deref()).as_str(),
        model_info.slug.as_str(),
        /*account_id*/ None,
        Some("test@test.com".to_string()),
        Some(TelemetryAuthMode::Chatgpt),
        "test_originator".to_string(),
        /*log_user_prompts*/ false,
        "test".to_string(),
        session_source,
    )
}

pub(crate) async fn make_session_configuration_for_tests() -> SessionConfiguration {
    let praxis_home = tempfile::tempdir().expect("create temp dir");
    let config = build_test_config(praxis_home.path()).await;
    let config = Arc::new(config);
    let model = ModelsManager::get_model_offline_for_tests(config.model.as_deref());
    let model_info = ModelsManager::construct_model_info_offline_for_tests(model.as_str(), &config);
    let reasoning_effort = config.model_reasoning_effort;
    let collaboration_mode = CollaborationMode {
        mode: ModeKind::Default,
        settings: Settings {
            model,
            reasoning_effort,
            developer_instructions: None,
        },
    };

    SessionConfiguration {
        provider: config.model_provider.clone(),
        collaboration_mode,
        model_reasoning_summary: config.model_reasoning_summary,
        developer_instructions: config.developer_instructions.clone(),
        user_instructions: config.user_instructions.clone(),
        service_tier: None,
        personality: config.personality,
        base_instructions: config
            .base_instructions
            .clone()
            .unwrap_or_else(|| model_info.get_model_instructions(config.personality)),
        compact_prompt: config.compact_prompt.clone(),
        approval_policy: config.permissions.approval_policy.clone(),
        approvals_reviewer: config.approvals_reviewer,
        sandbox_policy: config.permissions.sandbox_policy.clone(),
        file_system_sandbox_policy: config.permissions.file_system_sandbox_policy.clone(),
        network_sandbox_policy: config.permissions.network_sandbox_policy,
        windows_sandbox_level: WindowsSandboxLevel::from_config(&config),
        cwd: config.cwd.clone(),
        praxis_home: config.praxis_home.clone(),
        thread_name: None,
        original_config_do_not_use: Arc::clone(&config),
        metrics_service_name: None,
        app_gateway_client_name: None,
        session_source: SessionSource::Exec,
        dynamic_tools: Vec::new(),
        persist_extended_history: false,
        inherited_shell_snapshot: None,
        user_shell_override: None,
    }
}

pub(crate) async fn make_session_and_context() -> (Session, TurnContext) {
    let (tx_event, _rx_event) = async_channel::unbounded();
    let praxis_home = tempfile::tempdir().expect("create temp dir");
    let config = build_test_config(praxis_home.path()).await;
    let config = Arc::new(config);
    let conversation_id = ThreadId::default();
    let auth_manager =
        AuthManager::from_auth_for_testing(OpenAiAccountAuth::from_api_key("Test API Key"));
    let models_manager = Arc::new(ModelsManager::new(
        config.praxis_home.clone(),
        auth_manager.clone(),
        /*model_catalog*/ None,
        CollaborationModesConfig::default(),
    ));
    let agent_control = AgentControl::default();
    let exec_policy = Arc::new(ExecPolicyManager::default());
    let (agent_status_tx, _agent_status_rx) = watch::channel(AgentStatus::PendingInit);
    let model = ModelsManager::get_model_offline_for_tests(config.model.as_deref());
    let model_info = ModelsManager::construct_model_info_offline_for_tests(model.as_str(), &config);
    let reasoning_effort = config.model_reasoning_effort;
    let collaboration_mode = CollaborationMode {
        mode: ModeKind::Default,
        settings: Settings {
            model,
            reasoning_effort,
            developer_instructions: None,
        },
    };
    let session_configuration = SessionConfiguration {
        provider: config.model_provider.clone(),
        collaboration_mode,
        model_reasoning_summary: config.model_reasoning_summary,
        developer_instructions: config.developer_instructions.clone(),
        user_instructions: config.user_instructions.clone(),
        service_tier: None,
        personality: config.personality,
        base_instructions: config
            .base_instructions
            .clone()
            .unwrap_or_else(|| model_info.get_model_instructions(config.personality)),
        compact_prompt: config.compact_prompt.clone(),
        approval_policy: config.permissions.approval_policy.clone(),
        approvals_reviewer: config.approvals_reviewer,
        sandbox_policy: config.permissions.sandbox_policy.clone(),
        file_system_sandbox_policy: config.permissions.file_system_sandbox_policy.clone(),
        network_sandbox_policy: config.permissions.network_sandbox_policy,
        windows_sandbox_level: WindowsSandboxLevel::from_config(&config),
        cwd: config.cwd.clone(),
        praxis_home: config.praxis_home.clone(),
        thread_name: None,
        original_config_do_not_use: Arc::clone(&config),
        metrics_service_name: None,
        app_gateway_client_name: None,
        session_source: SessionSource::Exec,
        dynamic_tools: Vec::new(),
        persist_extended_history: false,
        inherited_shell_snapshot: None,
        user_shell_override: None,
    };
    let per_turn_config = Session::build_per_turn_config(&session_configuration);
    let model_info = ModelsManager::construct_model_info_offline_for_tests(
        session_configuration.collaboration_mode.model(),
        &per_turn_config,
    );
    let session_telemetry = session_telemetry(
        conversation_id,
        config.as_ref(),
        &model_info,
        session_configuration.session_source.clone(),
    );

    let state = SessionState::new(session_configuration.clone());
    let plugins_manager = Arc::new(PluginsManager::new(config.praxis_home.clone()));
    let mcp_manager = Arc::new(McpManager::new(Arc::clone(&plugins_manager)));
    let skills_manager = Arc::new(SkillsManager::new(
        config.praxis_home.clone(),
        /*bundled_skills_enabled*/ true,
    ));
    let network_approval = Arc::new(NetworkApprovalService::default());
    let environment = Arc::new(
        praxis_exec_server::Environment::create(/*exec_server_url*/ None)
            .await
            .expect("create environment"),
    );

    let skills_watcher = Arc::new(SkillsWatcher::noop());
    let services = SessionServices {
        mcp_connection_manager: Arc::new(RwLock::new(McpConnectionManager::new_uninitialized(
            &config.permissions.approval_policy,
        ))),
        mcp_startup_cancellation_token: Mutex::new(CancellationToken::new()),
        unified_exec_manager: Arc::new(UnifiedExecProcessManager::new(
            config.background_terminal_max_timeout,
        )),
        shell_zsh_path: None,
        main_execve_wrapper_exe: config.main_execve_wrapper_exe.clone(),
        analytics_events_client: AnalyticsEventsClient::new(
            Arc::clone(&auth_manager),
            config.chatgpt_base_url.trim_end_matches('/').to_string(),
            config.analytics_enabled,
        ),
        hooks: Hooks::new(HooksConfig {
            notify_argv: config.notify.clone(),
            ..HooksConfig::default()
        }),
        rollout: Mutex::new(None),
        user_shell: Arc::new(default_user_shell()),
        shell_snapshot_tx: watch::channel(None).0,
        show_raw_agent_reasoning: config.show_raw_agent_reasoning,
        exec_policy,
        auth_manager: auth_manager.clone(),
        session_telemetry: session_telemetry.clone(),
        models_manager: Arc::clone(&models_manager),
        tool_approvals: Mutex::new(ApprovalStore::default()),
        skills_manager,
        plugins_manager,
        mcp_manager,
        skills_watcher,
        agent_control,
        agent_os: crate::agent_os::AgentOs::new(),
        network_proxy: None,
        network_approval: Arc::clone(&network_approval),
        state_db: None,
        model_runtime: ModelRuntimeRegistry::new(
            Some(auth_manager.clone()),
            conversation_id,
            session_configuration.session_source.clone(),
            config.model_verbosity,
            config.features.enabled(Feature::EnableRequestCompression),
            config.features.enabled(Feature::RuntimeMetrics),
            Session::build_model_client_beta_features_header(config.as_ref()),
        ),
        code_mode_service: crate::tools::code_mode::CodeModeService::new(
            config.js_repl_node_path.clone(),
        ),
        environment: Arc::clone(&environment),
    };
    let js_repl = Arc::new(JsReplHandle::with_node_path(
        config.js_repl_node_path.clone(),
        config.js_repl_node_module_dirs.clone(),
    ));

    let plugin_outcome = services
        .plugins_manager
        .plugins_for_config(&per_turn_config);
    let effective_skill_roots = plugin_outcome.effective_skill_roots();
    let skills_input =
        crate::skills_load_input_from_config(&per_turn_config, effective_skill_roots);
    let skills_outcome = Arc::new(services.skills_manager.skills_for_config(&skills_input));
    let llm_runtime_catalog = crate::llm::runtime::LlmRuntimeCatalog::default();
    let turn_context = Session::make_turn_context(
        conversation_id,
        Some(Arc::clone(&auth_manager)),
        &session_telemetry,
        session_configuration.provider.clone(),
        &session_configuration,
        services.user_shell.as_ref(),
        services.shell_zsh_path.as_ref(),
        services.main_execve_wrapper_exe.as_ref(),
        per_turn_config,
        model_info,
        &models_manager,
        &llm_runtime_catalog,
        /*network*/ None,
        environment,
        "turn_id".to_string(),
        Arc::clone(&js_repl),
        skills_outcome,
    );

    let (mailbox, mailbox_rx) = crate::agent::Mailbox::new();
    let session = Session {
        conversation_id,
        tx_event,
        agent_status: agent_status_tx,
        out_of_band_elicitation_paused: watch::channel(false).0,
        state: Mutex::new(state),
        features: config.features.clone(),
        pending_mcp_server_refresh_config: Mutex::new(None),
        conversation: Arc::new(RealtimeConversationManager::new()),
        active_turn: Mutex::new(None),
        mailbox,
        mailbox_rx: Mutex::new(mailbox_rx),
        idle_pending_input: Mutex::new(Vec::new()),
        guardian_review_session: crate::guardian::GuardianReviewSessionManager::default(),
        services,
        goal_runtime: crate::goals::GoalRuntimeState::new(),
        llm_runtime_catalog,
        js_repl,
        next_internal_sub_id: AtomicU64::new(0),
        auto_title_attempted: AtomicBool::new(false),
        auto_summary_in_flight: AtomicBool::new(false),
    };

    (session, turn_context)
}

async fn sample_rollout(
    session: &Session,
    _turn_context: &TurnContext,
) -> (Vec<RolloutItem>, Vec<ResponseItem>) {
    let mut rollout_items = Vec::new();
    let mut live_history = ContextManager::new();

    // Use the same turn_context source as record_initial_history so model_info (and thus
    // personality_spec) matches reconstruction.
    let reconstruction_turn = session.new_default_turn().await;
    let mut initial_context = session
        .build_initial_context(reconstruction_turn.as_ref())
        .await;
    // Ensure personality_spec is present when Personality is enabled, so expected matches
    // what reconstruction produces (build_initial_context may omit it when baked into model).
    if !initial_context.iter().any(|m| {
        matches!(m, ResponseItem::Message { role, content, .. }
        if role == "developer"
            && content.iter().any(|c| {
                matches!(c, ContentItem::InputText { text } if text.contains("<personality_spec>"))
            }))
    }) && let Some(p) = reconstruction_turn.personality
        && session.features.enabled(Feature::Personality)
        && let Some(personality_message) = reconstruction_turn
            .model_info
            .model_messages
            .as_ref()
            .and_then(|m| m.get_personality_message(Some(p)).filter(|s| !s.is_empty()))
    {
        let msg = DeveloperInstructions::personality_spec_message(personality_message).into();
        let insert_at = initial_context
            .iter()
            .position(|m| matches!(m, ResponseItem::Message { role, .. } if role == "developer"))
            .map(|i| i + 1)
            .unwrap_or(0);
        initial_context.insert(insert_at, msg);
    }
    for item in &initial_context {
        rollout_items.push(RolloutItem::ResponseItem(item.clone()));
    }
    live_history.record_items(
        initial_context.iter(),
        reconstruction_turn.truncation_policy,
    );

    let user1 = ResponseItem::Message {
        id: None,
        role: "user".to_string(),
        content: vec![ContentItem::InputText {
            text: "first user".to_string(),
        }],
        end_turn: None,
        phase: None,
    };
    live_history.record_items(
        std::iter::once(&user1),
        reconstruction_turn.truncation_policy,
    );
    rollout_items.push(RolloutItem::ResponseItem(user1.clone()));

    let assistant1 = ResponseItem::Message {
        id: None,
        role: "assistant".to_string(),
        content: vec![ContentItem::OutputText {
            text: "assistant reply one".to_string(),
        }],
        end_turn: None,
        phase: None,
    };
    live_history.record_items(
        std::iter::once(&assistant1),
        reconstruction_turn.truncation_policy,
    );
    rollout_items.push(RolloutItem::ResponseItem(assistant1.clone()));

    let summary1 = "summary one";
    let snapshot1 = live_history
        .clone()
        .for_prompt(&reconstruction_turn.model_info.input_modalities);
    let user_messages1 = collect_user_messages(&snapshot1);
    let rebuilt1 = compact::build_compacted_history(Vec::new(), &user_messages1, summary1);
    live_history.replace(rebuilt1);
    rollout_items.push(RolloutItem::Compacted(CompactedItem {
        message: summary1.to_string(),
        replacement_history: None,
    }));

    let user2 = ResponseItem::Message {
        id: None,
        role: "user".to_string(),
        content: vec![ContentItem::InputText {
            text: "second user".to_string(),
        }],
        end_turn: None,
        phase: None,
    };
    live_history.record_items(
        std::iter::once(&user2),
        reconstruction_turn.truncation_policy,
    );
    rollout_items.push(RolloutItem::ResponseItem(user2.clone()));

    let assistant2 = ResponseItem::Message {
        id: None,
        role: "assistant".to_string(),
        content: vec![ContentItem::OutputText {
            text: "assistant reply two".to_string(),
        }],
        end_turn: None,
        phase: None,
    };
    live_history.record_items(
        std::iter::once(&assistant2),
        reconstruction_turn.truncation_policy,
    );
    rollout_items.push(RolloutItem::ResponseItem(assistant2.clone()));

    let summary2 = "summary two";
    let snapshot2 = live_history
        .clone()
        .for_prompt(&reconstruction_turn.model_info.input_modalities);
    let user_messages2 = collect_user_messages(&snapshot2);
    let rebuilt2 = compact::build_compacted_history(Vec::new(), &user_messages2, summary2);
    live_history.replace(rebuilt2);
    rollout_items.push(RolloutItem::Compacted(CompactedItem {
        message: summary2.to_string(),
        replacement_history: None,
    }));

    let user3 = ResponseItem::Message {
        id: None,
        role: "user".to_string(),
        content: vec![ContentItem::InputText {
            text: "third user".to_string(),
        }],
        end_turn: None,
        phase: None,
    };
    live_history.record_items(
        std::iter::once(&user3),
        reconstruction_turn.truncation_policy,
    );
    rollout_items.push(RolloutItem::ResponseItem(user3));

    let assistant3 = ResponseItem::Message {
        id: None,
        role: "assistant".to_string(),
        content: vec![ContentItem::OutputText {
            text: "assistant reply three".to_string(),
        }],
        end_turn: None,
        phase: None,
    };
    live_history.record_items(
        std::iter::once(&assistant3),
        reconstruction_turn.truncation_policy,
    );
    rollout_items.push(RolloutItem::ResponseItem(assistant3));

    (
        rollout_items,
        live_history.for_prompt(&reconstruction_turn.model_info.input_modalities),
    )
}
