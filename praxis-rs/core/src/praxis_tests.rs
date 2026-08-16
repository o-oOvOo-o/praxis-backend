pub(super) use super::model_request::collect_explicit_app_ids_from_skill_items;
pub(super) use super::model_request::filter_connectors_for_input;
pub(super) use super::model_request::filter_praxis_apps_mcp_tools;
pub(super) use super::*;
pub(super) use crate::config::ConfigBuilder;
pub(super) use crate::config::test_config;
pub(super) use crate::config_loader::ConfigLayerStack;
pub(super) use crate::config_loader::ConfigLayerStackOrdering;
pub(super) use crate::config_loader::NetworkConstraints;
pub(super) use crate::config_loader::NetworkDomainPermissionToml;
pub(super) use crate::config_loader::NetworkDomainPermissionsToml;
pub(super) use crate::config_loader::RequirementSource;
pub(super) use crate::config_loader::Sourced;
pub(super) use crate::exec::ExecCapturePolicy;
pub(super) use crate::exec::ExecToolCallOutput;
pub(super) use crate::function_tool::FunctionCallError;
pub(super) use crate::models_manager::model_info;
pub(super) use crate::shell::default_user_shell;
pub(super) use crate::tools::format_exec_output_str;

pub(super) use praxis_features::Features;
pub(super) use praxis_login::OpenAiAccountAuth;
pub(super) use praxis_mcp::mcp_connection_manager::ToolInfo;
pub(super) use praxis_protocol::ThreadId;
pub(super) use praxis_protocol::models::FunctionCallOutputBody;
pub(super) use praxis_protocol::models::FunctionCallOutputPayload;
pub(super) use praxis_protocol::permissions::FileSystemAccessMode;
pub(super) use praxis_protocol::permissions::FileSystemPath;
pub(super) use praxis_protocol::permissions::FileSystemSandboxEntry;
pub(super) use praxis_protocol::permissions::FileSystemSandboxPolicy;
pub(super) use praxis_protocol::permissions::FileSystemSpecialPath;
pub(super) use praxis_protocol::protocol::NonSteerableTurnKind;
pub(super) use praxis_protocol::protocol::ReadOnlyAccess;
pub(super) use praxis_protocol::protocol::SandboxPolicy;
pub(super) use praxis_protocol::request_permissions::PermissionGrantScope;
pub(super) use praxis_protocol::request_permissions::RequestPermissionProfile;
pub(super) use tracing::Instrument;
pub(super) use tracing::Span;

pub(super) use crate::rollout::policy::EventPersistenceMode;
pub(super) use crate::rollout::recorder::RolloutRecorder;
pub(super) use crate::rollout::recorder::RolloutRecorderParams;
pub(super) use crate::state::AgentTaskKind;
pub(super) use crate::state::SessionTokenLedger;
pub(super) use crate::tasks::AgentTask;
pub(super) use crate::tasks::AgentTaskContext;
pub(super) use crate::tools::ToolRouter;
pub(super) use crate::tools::context::FunctionToolOutput;
pub(super) use crate::tools::context::ToolInvocation;
pub(super) use crate::tools::context::ToolPayload;
pub(super) use crate::tools::handlers::ShellHandler;
pub(super) use crate::tools::handlers::UnifiedExecHandler;
pub(super) use crate::tools::registry::ToolHandler;
pub(super) use crate::tools::router::ToolCallSource;
pub(super) use crate::turn_completed_output::CompletedOutputCtx;
pub(super) use crate::turn_completed_output::handle_completed_output_item;
pub(super) use crate::turn_diff_tracker::TurnDiffTracker;
pub(super) use core_test_support::PathBufExt;
pub(super) use core_test_support::context_snapshot;
pub(super) use core_test_support::context_snapshot::ContextSnapshotOptions;
pub(super) use core_test_support::context_snapshot::ContextSnapshotRenderMode;
pub(super) use core_test_support::responses::ev_completed;
pub(super) use core_test_support::responses::ev_response_created;
pub(super) use core_test_support::responses::mount_sse_once;
pub(super) use core_test_support::responses::sse;
pub(super) use core_test_support::responses::start_mock_server;
pub(super) use core_test_support::test_praxis::test_praxis;
pub(super) use core_test_support::tracing::install_test_tracing;
pub(super) use core_test_support::wait_for_event;
pub(super) use opentelemetry::trace::TraceContextExt;
pub(super) use opentelemetry::trace::TraceId;
pub(super) use praxis_execpolicy::Decision;
pub(super) use praxis_execpolicy::NetworkRuleProtocol;
pub(super) use praxis_execpolicy::Policy;
pub(super) use praxis_network_proxy::NetworkProxyConfig;
pub(super) use praxis_otel::TelemetryAuthMode;
pub(super) use praxis_protocol::apps::AppInfo;
pub(super) use praxis_protocol::config_types::CollaborationMode;
pub(super) use praxis_protocol::config_types::ModeKind;
pub(super) use praxis_protocol::config_types::Settings;
pub(super) use praxis_protocol::models::BaseInstructions;
pub(super) use praxis_protocol::models::ContentItem;
pub(super) use praxis_protocol::models::DeveloperInstructions;
pub(super) use praxis_protocol::models::ResponseInputItem;
pub(super) use praxis_protocol::models::ResponseItem;
pub(super) use praxis_protocol::openai_models::ModelsResponse;
pub(super) use praxis_protocol::protocol::AskForApproval;
pub(super) use praxis_protocol::protocol::CompactedItem;
pub(super) use praxis_protocol::protocol::ConversationAudioParams;
pub(super) use praxis_protocol::protocol::CreditsSnapshot;
pub(super) use praxis_protocol::protocol::GranularApprovalConfig;
pub(super) use praxis_protocol::protocol::InitialHistory;
pub(super) use praxis_protocol::protocol::NetworkApprovalProtocol;
pub(super) use praxis_protocol::protocol::RateLimitSnapshot;
pub(super) use praxis_protocol::protocol::RateLimitWindow;
pub(super) use praxis_protocol::protocol::RealtimeAudioFrame;
pub(super) use praxis_protocol::protocol::ResumedHistory;
pub(super) use praxis_protocol::protocol::RolloutItem;
pub(super) use praxis_protocol::protocol::Submission;
pub(super) use praxis_protocol::protocol::ThreadRolledBackEvent;
pub(super) use praxis_protocol::protocol::TokenCountEvent;
pub(super) use praxis_protocol::protocol::TokenUsage;
pub(super) use praxis_protocol::protocol::TokenUsageInfo;
pub(super) use praxis_protocol::protocol::TurnAbortedEvent;
pub(super) use praxis_protocol::protocol::TurnCompleteEvent;
pub(super) use praxis_protocol::protocol::TurnStartedEvent;
pub(super) use praxis_protocol::protocol::UserMessageEvent;
pub(super) use praxis_protocol::protocol::W3cTraceContext;
pub(super) use std::path::Path;
pub(super) use std::time::Duration;
pub(super) use tokio::time::sleep;
pub(super) use tracing_opentelemetry::OpenTelemetrySpanExt;

pub(super) use praxis_protocol::mcp::CallToolResult as McpCallToolResult;
pub(super) use rmcp::model::JsonObject;
pub(super) use rmcp::model::Tool;
pub(super) use serde::Deserialize;
pub(super) use serde_json::json;
pub(super) use std::path::PathBuf;
pub(super) use std::sync::Arc;
pub(super) use std::time::Duration as StdDuration;

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
pub(crate) use shutdown_and_tasks::make_session_and_context_with_dynamic_tools_and_rx;
pub(crate) use shutdown_and_tasks::make_session_and_context_with_rx;

pub(super) use praxis_protocol::models::function_call_output_content_items_to_text;

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
        requested_thread_id: None,
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
        requested_thread_id: None,
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
    let permission_ledger =
        PermissionLedger::from_session_configuration(&conversation_id, &session_configuration);
    let effective_permissions = permission_ledger.live_effective_permissions();
    let token_ledger = SessionTokenLedger::from_state(&state);
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
    let (capability_scope, hook_capability) = crate::capabilities::test_hook_capability(
        conversation_id,
        Hooks::new(HooksConfig {
            notify_argv: config.notify.clone(),
            ..HooksConfig::default()
        }),
    );
    let provider_capability = crate::capabilities::publish_providers(
        &capability_scope.runtime(),
        Arc::clone(&models_manager),
    )
    .expect("publish test Providers capability");
    let skills_manager =
        crate::capabilities::publish_skills(&capability_scope.runtime(), skills_manager)
            .expect("publish test Skills capability");
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
        hook_capability,
        _capability_scope: capability_scope,
        rollout: Mutex::new(None),
        user_shell: Arc::new(default_user_shell()),
        shell_snapshot_tx: watch::channel(None).0,
        show_raw_agent_reasoning: config.show_raw_agent_reasoning,
        exec_policy,
        auth_manager: auth_manager.clone(),
        session_telemetry: session_telemetry.clone(),
        models_manager: provider_capability,
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
            None,
            crate::llm::local_models::NativeLocalModelConfig::from_config(config.as_ref()),
        ),
        code_mode_service: crate::tools::code_mode::CodeModeService::new(),
        environment: Arc::clone(&environment),
    };

    let plugin_outcome = services
        .plugins_manager
        .plugins_for_config(&per_turn_config);
    let effective_skill_roots = plugin_outcome.effective_skill_roots();
    let skills_input =
        crate::skills_load_input_from_config(&per_turn_config, effective_skill_roots);
    let skills_outcome = crate::capabilities::publish_resolved_skills(
        &services._capability_scope,
        conversation_id,
        "turn_id",
        services.skills_manager.skills_for_config(&skills_input),
    )
    .expect("publish test turn resolved Skills capability");
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
        effective_permissions,
        skills_outcome,
    );

    let (mailbox, mailbox_rx) = crate::agent::Mailbox::new();
    let session = Session {
        conversation_id,
        tx_event,
        agent_status: agent_status_tx,
        out_of_band_elicitation_paused: watch::channel(false).0,
        permission_ledger,
        state: Mutex::new(state),
        token_ledger: RwLock::new(token_ledger),
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
        context_governance: Default::default(),
        llm_runtime_catalog,
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
