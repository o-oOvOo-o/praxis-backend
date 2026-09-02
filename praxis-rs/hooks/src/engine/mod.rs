pub(crate) mod command_runner;
pub(crate) mod config;
pub(crate) mod discovery;
pub(crate) mod dispatcher;
pub(crate) mod output_parser;
pub(crate) mod schema_loader;

use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::sync::Arc;

use praxis_config::ConfigLayerStack;
use praxis_protocol::protocol::HookRunSummary;

use crate::events::post_tool_use::PostToolUseOutcome;
use crate::events::post_tool_use::PostToolUseRequest;
use crate::events::pre_tool_use::PreToolUseOutcome;
use crate::events::pre_tool_use::PreToolUseRequest;
use crate::events::session_start::SessionStartOutcome;
use crate::events::session_start::SessionStartRequest;
use crate::events::stop::StopOutcome;
use crate::events::stop::StopRequest;
use crate::events::user_prompt_submit::UserPromptSubmitOutcome;
use crate::events::user_prompt_submit::UserPromptSubmitRequest;

#[derive(Debug, Clone)]
pub(crate) struct CommandShell {
    pub program: String,
    pub args: Vec<String>,
    pub concurrency: NonZeroUsize,
}

#[derive(Debug, Clone)]
pub(crate) struct ConfiguredHandler {
    pub event_name: praxis_protocol::protocol::HookEventName,
    pub matcher: Option<String>,
    pub matcher_regex: Option<regex::Regex>,
    pub command: String,
    pub timeout_sec: u64,
    pub status_message: Option<String>,
    pub source_path: PathBuf,
    pub display_order: i64,
}

impl PartialEq for ConfiguredHandler {
    fn eq(&self, other: &Self) -> bool {
        self.event_name == other.event_name
            && self.matcher == other.matcher
            && self.command == other.command
            && self.timeout_sec == other.timeout_sec
            && self.status_message == other.status_message
            && self.source_path == other.source_path
            && self.display_order == other.display_order
    }
}

impl Eq for ConfiguredHandler {}

impl ConfiguredHandler {
    pub fn matches(&self, input: Option<&str>) -> bool {
        match &self.matcher_regex {
            Some(matcher) => input.is_some_and(|input| matcher.is_match(input)),
            None => true,
        }
    }

    pub fn run_id(&self) -> String {
        format!(
            "{}:{}:{}",
            self.event_name_label(),
            self.display_order,
            self.source_path.display()
        )
    }

    fn event_name_label(&self) -> &'static str {
        match self.event_name {
            praxis_protocol::protocol::HookEventName::PreToolUse => "pre-tool-use",
            praxis_protocol::protocol::HookEventName::PostToolUse => "post-tool-use",
            praxis_protocol::protocol::HookEventName::SessionStart => "session-start",
            praxis_protocol::protocol::HookEventName::UserPromptSubmit => "user-prompt-submit",
            praxis_protocol::protocol::HookEventName::Stop => "stop",
        }
    }
}

#[derive(Clone)]
pub(crate) struct CommandHookAdapter {
    handlers: Arc<[ConfiguredHandler]>,
    warnings: Arc<[String]>,
    shell: Arc<CommandShell>,
}

impl CommandHookAdapter {
    pub(crate) fn new(
        enabled: bool,
        config_layer_stack: Option<&ConfigLayerStack>,
        shell: CommandShell,
    ) -> Self {
        if !enabled {
            return Self {
                handlers: Arc::from([]),
                warnings: Arc::from([]),
                shell: Arc::new(shell),
            };
        }

        if cfg!(windows) {
            return Self {
                handlers: Arc::from([]),
                warnings: Arc::from([
                    "Disabled `praxis_hooks` for this session because `hooks.json` lifecycle hooks are not supported on Windows yet."
                        .to_string(),
                ]),
                shell: Arc::new(shell),
            };
        }

        let _ = schema_loader::generated_hook_schemas();
        let discovered = discovery::discover_handlers(config_layer_stack);
        Self {
            handlers: discovered.handlers.into(),
            warnings: discovered.warnings.into(),
            shell: Arc::new(shell),
        }
    }

    pub(crate) fn warnings(&self) -> &[String] {
        &self.warnings
    }

    pub(crate) fn preview_session_start(
        &self,
        request: &SessionStartRequest,
    ) -> Vec<HookRunSummary> {
        crate::events::session_start::preview(&self.handlers, request)
    }

    pub(crate) fn preview_pre_tool_use(&self, request: &PreToolUseRequest) -> Vec<HookRunSummary> {
        crate::events::pre_tool_use::preview(&self.handlers, request)
    }

    pub(crate) fn preview_post_tool_use(
        &self,
        request: &PostToolUseRequest,
    ) -> Vec<HookRunSummary> {
        crate::events::post_tool_use::preview(&self.handlers, request)
    }

    pub(crate) async fn run_session_start(
        &self,
        request: SessionStartRequest,
        turn_id: Option<String>,
    ) -> SessionStartOutcome {
        crate::events::session_start::run(&self.handlers, &self.shell, request, turn_id).await
    }

    pub(crate) async fn run_pre_tool_use(&self, request: PreToolUseRequest) -> PreToolUseOutcome {
        crate::events::pre_tool_use::run(&self.handlers, &self.shell, request).await
    }

    pub(crate) async fn run_post_tool_use(
        &self,
        request: PostToolUseRequest,
    ) -> PostToolUseOutcome {
        crate::events::post_tool_use::run(&self.handlers, &self.shell, request).await
    }

    pub(crate) fn preview_user_prompt_submit(
        &self,
        request: &UserPromptSubmitRequest,
    ) -> Vec<HookRunSummary> {
        crate::events::user_prompt_submit::preview(&self.handlers, request)
    }

    pub(crate) async fn run_user_prompt_submit(
        &self,
        request: UserPromptSubmitRequest,
    ) -> UserPromptSubmitOutcome {
        crate::events::user_prompt_submit::run(&self.handlers, &self.shell, request).await
    }

    pub(crate) fn preview_stop(&self, request: &StopRequest) -> Vec<HookRunSummary> {
        crate::events::stop::preview(&self.handlers, request)
    }

    pub(crate) async fn run_stop(&self, request: StopRequest) -> StopOutcome {
        crate::events::stop::run(&self.handlers, &self.shell, request).await
    }
}
