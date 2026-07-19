pub(super) use super::*;
pub(super) use crate::PraxisThread;
pub(super) use crate::ThreadManager;
pub(super) use crate::agent::agent_status_from_event;
pub(super) use crate::config::AgentRoleConfig;
pub(super) use crate::config::Config;
pub(super) use crate::config::ConfigBuilder;
pub(super) use crate::config_loader::LoaderOverrides;
pub(super) use crate::contextual_user_message::SUBAGENT_NOTIFICATION_OPEN_TAG;
pub(super) use assert_matches::assert_matches;
pub(super) use chrono::Utc;
pub(super) use praxis_features::Feature;
pub(super) use praxis_login::OpenAiAccountAuth;
pub(super) use praxis_protocol::AgentPath;
pub(super) use praxis_protocol::config_types::ApprovalsReviewer;
pub(super) use praxis_protocol::config_types::ModeKind;
pub(super) use praxis_protocol::models::ContentItem;
pub(super) use praxis_protocol::models::ResponseItem;
pub(super) use praxis_protocol::protocol::AskForApproval;
pub(super) use praxis_protocol::protocol::ErrorEvent;
pub(super) use praxis_protocol::protocol::EventMsg;
pub(super) use praxis_protocol::protocol::InterAgentCommunication;
pub(super) use praxis_protocol::protocol::SandboxPolicy;
pub(super) use praxis_protocol::protocol::SessionSource;
pub(super) use praxis_protocol::protocol::SubAgentSource;
pub(super) use praxis_protocol::protocol::TurnAbortReason;
pub(super) use praxis_protocol::protocol::TurnAbortedEvent;
pub(super) use praxis_protocol::protocol::TurnCompleteEvent;
pub(super) use praxis_protocol::protocol::TurnStartedEvent;
pub(super) use std::path::PathBuf;
pub(super) use tempfile::TempDir;
pub(super) use tokio::time::Duration;
pub(super) use tokio::time::sleep;
pub(super) use tokio::time::timeout;
pub(super) use toml::Value as TomlValue;

async fn test_config_with_cli_overrides(
    cli_overrides: Vec<(String, TomlValue)>,
) -> (TempDir, Config) {
    let home = TempDir::new().expect("create temp dir");
    let config = ConfigBuilder::default()
        .praxis_home(home.path().to_path_buf())
        .cli_overrides(cli_overrides)
        .loader_overrides(LoaderOverrides {
            macos_managed_config_requirements_base64: Some(String::new()),
            ..LoaderOverrides::default()
        })
        .build()
        .await
        .expect("load default test config");
    (home, config)
}

async fn test_config() -> (TempDir, Config) {
    test_config_with_cli_overrides(Vec::new()).await
}

fn text_input(text: &str) -> Op {
    Op::UserTurn {
        items: vec![UserInput::Text {
            text: text.to_string(),
            text_elements: Vec::new(),
        }],
        cwd: PathBuf::from("."),
        approval_policy: AskForApproval::Never,
        approvals_reviewer: Some(ApprovalsReviewer::User),
        sandbox_policy: SandboxPolicy::DangerFullAccess,
        model: "gpt-5".to_string(),
        model_provider: Some("openai".to_string()),
        effort: None,
        summary: None,
        service_tier: None,
        final_output_json_schema: None,
        collaboration_mode: None,
        personality: None,
    }
}

#[path = "control_tests/completion_notifications.rs"]
mod completion_notifications;
#[path = "control_tests/display_names.rs"]
mod display_names;
#[path = "control_tests/messaging.rs"]
mod messaging;
#[path = "control_tests/spawn_fork.rs"]
mod spawn_fork;
#[path = "control_tests/status_and_lifecycle.rs"]
mod status_and_lifecycle;
#[path = "control_tests/subagent_identity_resume.rs"]
mod subagent_identity_resume;
#[path = "control_tests/thread_limits.rs"]
mod thread_limits;
#[path = "control_tests/tree_resume_shutdown.rs"]
mod tree_resume_shutdown;

struct AgentControlHarness {
    _home: TempDir,
    config: Config,
    manager: ThreadManager,
    control: AgentControl,
}

impl AgentControlHarness {
    async fn new() -> Self {
        let (home, config) = test_config().await;
        let manager = ThreadManager::with_models_provider_and_home_for_tests(
            OpenAiAccountAuth::from_api_key("dummy"),
            config.model_provider.clone(),
            config.praxis_home.clone(),
            std::sync::Arc::new(praxis_exec_server::EnvironmentManager::new(
                /*exec_server_url*/ None,
            )),
        );
        let control = manager.agent_control();
        Self {
            _home: home,
            config,
            manager,
            control,
        }
    }

    async fn start_thread(&self) -> (ThreadId, Arc<PraxisThread>) {
        let new_thread = self
            .manager
            .start_thread(self.config.clone())
            .await
            .expect("start thread");
        (new_thread.thread_id, new_thread.thread)
    }
}

fn has_subagent_notification(history_items: &[ResponseItem]) -> bool {
    history_items.iter().any(|item| {
        let ResponseItem::Message { role, content, .. } = item else {
            return false;
        };
        if role != "user" {
            return false;
        }
        content.iter().any(|content_item| match content_item {
            ContentItem::InputText { text } | ContentItem::OutputText { text } => {
                text.contains(SUBAGENT_NOTIFICATION_OPEN_TAG)
            }
            ContentItem::InputImage { .. } => false,
        })
    })
}

/// Returns true when any message item contains `needle` in a text span.
fn history_contains_text(history_items: &[ResponseItem], needle: &str) -> bool {
    history_items.iter().any(|item| {
        let ResponseItem::Message { content, .. } = item else {
            return false;
        };
        content.iter().any(|content_item| match content_item {
            ContentItem::InputText { text } | ContentItem::OutputText { text } => {
                text.contains(needle)
            }
            ContentItem::InputImage { .. } => false,
        })
    })
}

fn history_contains_assistant_inter_agent_communication(
    history_items: &[ResponseItem],
    expected: &InterAgentCommunication,
) -> bool {
    history_items.iter().any(|item| {
        let ResponseItem::Message { role, content, .. } = item else {
            return false;
        };
        if role != "assistant" {
            return false;
        }
        content.iter().any(|content_item| match content_item {
            ContentItem::OutputText { text } => {
                serde_json::from_str::<InterAgentCommunication>(text)
                    .ok()
                    .as_ref()
                    == Some(expected)
            }
            ContentItem::InputText { .. } | ContentItem::InputImage { .. } => false,
        })
    })
}

async fn wait_for_subagent_notification(parent_thread: &Arc<PraxisThread>) -> bool {
    let wait = async {
        loop {
            let history_items = parent_thread
                .praxis
                .session
                .clone_history()
                .await
                .raw_items()
                .to_vec();
            if has_subagent_notification(&history_items) {
                return true;
            }
            sleep(Duration::from_millis(25)).await;
        }
    };
    timeout(Duration::from_secs(2), wait).await.is_ok()
}

async fn persist_thread_for_tree_resume(thread: &Arc<PraxisThread>, message: &str) {
    thread
        .inject_user_message_without_turn(message.to_string())
        .await;
    thread.praxis.session.ensure_rollout_materialized().await;
    thread.praxis.session.flush_rollout().await;
}

async fn wait_for_live_thread_spawn_children(
    control: &AgentControl,
    parent_thread_id: ThreadId,
    expected_children: &[ThreadId],
) {
    let mut expected_children = expected_children.to_vec();
    expected_children.sort_by_key(std::string::ToString::to_string);

    timeout(Duration::from_secs(5), async {
        loop {
            let mut child_ids = control
                .open_thread_spawn_children(parent_thread_id)
                .await
                .expect("live child list should load")
                .into_iter()
                .map(|(thread_id, _)| thread_id)
                .collect::<Vec<_>>();
            child_ids.sort_by_key(std::string::ToString::to_string);
            if child_ids == expected_children {
                break;
            }
            sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .expect("expected persisted child tree");
}
