use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;

use async_trait::async_trait;
use futures::stream;
use praxis_loop::LoopGuard;
use praxis_loop::ToolCallAdmission;
use praxis_loop::TurnContext;
use praxis_loop::TurnError;
use praxis_loop::TurnErrorKind;
use praxis_loop::TurnHooks;
use praxis_loop::TurnInput;
use praxis_loop::TurnResult;
use praxis_loop::TurnState;
use praxis_loop::decisions::SteeringDecision;
use praxis_loop::decisions::SteeringInputView;
use praxis_loop::decisions::ToolCallView;
use praxis_loop::decisions::ToolDecision;
use praxis_loop::decisions::ToolResultDecision;
use praxis_loop::decisions::ToolResultView;
use praxis_loop::ids::ThreadId;
use praxis_loop::ids::TraceId;
use praxis_loop::ids::TurnId;
use praxis_loop::model::ModelEvent;
use praxis_loop::model::ModelSpec;
use praxis_loop::model::PromptItem;
use praxis_loop::model::SteeringMessage;
use praxis_loop::model::TokenUsage;
use praxis_loop::model::TurnEvent;
use praxis_loop::model::TurnItem;
use praxis_loop::outcome::LoopResult;
use praxis_loop::run_turn;
use praxis_loop::services::EventSink;
use praxis_loop::services::HistorySink;
use praxis_loop::services::ModelEventStream;
use praxis_loop::services::ModelRequest;
use praxis_loop::services::ModelService;
use praxis_loop::services::SteeringControl;
use praxis_loop::services::SteeringDrain;
use praxis_loop::services::SteeringInbox;
use praxis_loop::services::ToolAccess;
use praxis_loop::tool::ConcurrencyMode;
use praxis_loop::tool::Tool;
use praxis_loop::tool::ToolCall;
use praxis_loop::tool::ToolResult;
use praxis_loop::tool::ToolSpec;
use tokio_util::sync::CancellationToken;

#[derive(Default)]
struct MockServices {
    streams: Mutex<Vec<Vec<ModelEvent>>>,
    requests: Mutex<Vec<ModelRequest>>,
    events: Mutex<Vec<TurnEvent>>,
    persisted: Mutex<Vec<TurnItem>>,
    steering: Mutex<Option<SteeringDrain>>,
    tools: Mutex<HashMap<String, Arc<dyn Tool>>>,
}

impl MockServices {
    fn with_streams(streams: Vec<Vec<ModelEvent>>) -> Self {
        Self {
            streams: Mutex::new(streams),
            requests: Mutex::new(Vec::new()),
            events: Mutex::new(Vec::new()),
            persisted: Mutex::new(Vec::new()),
            steering: Mutex::new(None),
            tools: Mutex::new(HashMap::new()),
        }
    }

    fn with_steering(self, drain: SteeringDrain) -> Self {
        *self.steering.lock().expect("steering lock") = Some(drain);
        self
    }

    fn insert_tool(&self, tool: Arc<dyn Tool>) {
        self.tools
            .lock()
            .expect("tool registry lock")
            .insert(tool.spec().name, tool);
    }

    fn persisted(&self) -> Vec<TurnItem> {
        self.persisted.lock().expect("persisted lock").clone()
    }

    fn requests(&self) -> Vec<ModelRequest> {
        self.requests.lock().expect("requests lock").clone()
    }
}

#[async_trait]
impl ModelService for MockServices {
    async fn stream_model(
        &self,
        request: ModelRequest,
        _cancel: CancellationToken,
    ) -> LoopResult<ModelEventStream> {
        self.requests.lock().expect("requests lock").push(request);
        let events = {
            let mut streams = self.streams.lock().expect("streams lock");
            if streams.is_empty() {
                return Err(TurnError::new(TurnErrorKind::Model, "missing mock stream"));
            }
            streams.remove(0)
        };
        Ok(Box::pin(stream::iter(events.into_iter().map(Ok))))
    }
}

#[async_trait]
impl EventSink for MockServices {
    async fn emit_event(&self, event: TurnEvent) -> LoopResult<()> {
        self.events.lock().expect("events lock").push(event);
        Ok(())
    }
}

#[async_trait]
impl HistorySink for MockServices {
    async fn persist_items(&self, items: &[TurnItem]) -> LoopResult<()> {
        self.persisted
            .lock()
            .expect("persisted lock")
            .extend_from_slice(items);
        Ok(())
    }
}

#[async_trait]
impl SteeringInbox for MockServices {
    async fn drain_steering(&self) -> LoopResult<SteeringDrain> {
        Ok(self
            .steering
            .lock()
            .expect("steering lock")
            .take()
            .unwrap_or_else(SteeringDrain::empty))
    }

    async fn wait_for_steering(&self) -> LoopResult<()> {
        std::future::pending().await
    }
}

impl ToolAccess for MockServices {
    fn resolve_tool(&self, name: &str) -> Option<Arc<dyn Tool>> {
        self.tools.lock().expect("tools lock").get(name).cloned()
    }
}

struct RecordingTool {
    name: String,
    mode: ConcurrencyMode,
    log: Arc<Mutex<Vec<String>>>,
    delay_ms: u64,
}

#[async_trait]
impl Tool for RecordingTool {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: self.name.clone(),
            description: String::new(),
            concurrency: self.mode,
        }
    }

    async fn execute(&self, call: ToolCall, _cancel: CancellationToken) -> LoopResult<ToolResult> {
        self.log
            .lock()
            .expect("log lock")
            .push(format!("start:{}", call.id));
        if self.delay_ms > 0 {
            tokio::time::sleep(Duration::from_millis(self.delay_ms)).await;
        }
        self.log
            .lock()
            .expect("log lock")
            .push(format!("finish:{}", call.id));
        Ok(ToolResult::success(call.id, self.name.clone()))
    }
}

fn test_context() -> TurnContext {
    TurnContext::new(
        TurnId::new("turn-1"),
        ThreadId::new("thread-1"),
        TraceId::new("trace-1"),
        ModelSpec::new("mock-model"),
    )
}

fn text_stream(text: &str) -> Vec<ModelEvent> {
    vec![
        ModelEvent::TextDelta {
            item_id: None,
            text: text.to_string(),
        },
        ModelEvent::Completed(TokenUsage {
            input: 1,
            output: 1,
            total: 2,
            reasoning: 0,
        }),
    ]
}

#[test]
fn empty_turn_item_records_do_not_mark_diff_changed() {
    let mut state = TurnState::default();

    state.record_items(Vec::<TurnItem>::new());
    assert!(!state.diff().has_changes());
    assert!(state.transcript_delta().is_empty());

    state.record_items([TurnItem::UserText("real change".to_string())]);
    assert!(state.diff().has_changes());
    assert_eq!(state.transcript_delta().len(), 1);
}

#[test]
fn tool_call_admission_has_explicit_rejection_state() {
    let mut state = TurnState::default().with_guard(LoopGuard::with_max_tool_calls(1));

    assert_eq!(state.record_tool_calls(1), ToolCallAdmission::Accepted);
    assert_eq!(
        state.record_tool_calls(1),
        ToolCallAdmission::Rejected {
            attempted_tool_call_count: 2
        }
    );
    assert_eq!(state.tool_call_count(), 2);
}

#[test]
fn loop_guard_owns_tool_call_admission_decision() {
    let guard = LoopGuard::with_max_tool_calls(1);

    assert_eq!(guard.admit_tool_calls(1), ToolCallAdmission::Accepted);
    assert_eq!(
        guard.admit_tool_calls(2),
        ToolCallAdmission::Rejected {
            attempted_tool_call_count: 2
        }
    );
}

#[tokio::test]
async fn final_answer_completes_and_persists_transcript() {
    let services = MockServices::with_streams(vec![text_stream("done")]);
    let result = run_turn(
        test_context(),
        TurnState::default(),
        &services,
        &praxis_loop::NoopHooks,
        TurnInput::from_prompt_items(vec![PromptItem::UserText("go".to_string())]),
        CancellationToken::new(),
    )
    .await;

    match result {
        TurnResult::Complete { state } => {
            assert_eq!(state.last_agent_message(), Some("done"));
            assert_eq!(state.token_usage().cumulative.total, 2);
        }
        other => panic!("unexpected result: {other:?}"),
    }

    assert_eq!(
        services.persisted(),
        vec![TurnItem::AssistantText {
            item_id: None,
            text: "done".to_string()
        }]
    );
}

#[tokio::test]
async fn steering_messages_are_injected_by_explicit_decision() {
    let services =
        MockServices::with_streams(vec![text_stream("done")]).with_steering(SteeringDrain {
            messages: vec![SteeringMessage::new(vec![PromptItem::UserText(
                "steer".to_string(),
            )])],
            control: SteeringControl::Continue,
        });

    let result = run_turn(
        test_context(),
        TurnState::default(),
        &services,
        &praxis_loop::NoopHooks,
        TurnInput::from_prompt_items(vec![PromptItem::UserText("go".to_string())]),
        CancellationToken::new(),
    )
    .await;

    assert!(matches!(result, TurnResult::Complete { .. }));
    assert!(
        services.requests()[0]
            .prompt
            .iter()
            .any(|item| matches!(item, PromptItem::UserText(text) if text == "steer"))
    );
}

struct DropSteeringHooks;

#[async_trait]
impl TurnHooks for DropSteeringHooks {
    async fn on_steering_input(&self, _view: SteeringInputView<'_>) -> SteeringDecision {
        SteeringDecision::DropAndContinue
    }
}

#[tokio::test]
async fn steering_messages_can_be_dropped_by_explicit_decision() {
    let services =
        MockServices::with_streams(vec![text_stream("done")]).with_steering(SteeringDrain {
            messages: vec![SteeringMessage::new(vec![PromptItem::UserText(
                "drop me".to_string(),
            )])],
            control: SteeringControl::Continue,
        });

    let result = run_turn(
        test_context(),
        TurnState::default(),
        &services,
        &DropSteeringHooks,
        TurnInput::from_prompt_items(vec![PromptItem::UserText("go".to_string())]),
        CancellationToken::new(),
    )
    .await;

    assert!(matches!(result, TurnResult::Complete { .. }));
    assert!(
        !services.requests()[0]
            .prompt
            .iter()
            .any(|item| matches!(item, PromptItem::UserText(text) if text == "drop me"))
    );
}

#[tokio::test]
async fn tool_batches_keep_exclusive_calls_between_parallel_groups() {
    let log = Arc::new(Mutex::new(Vec::new()));
    let services = MockServices::with_streams(vec![
        vec![
            ModelEvent::ToolCall(ToolCall::new("p1", "parallel")),
            ModelEvent::ToolCall(ToolCall::new("p2", "parallel")),
            ModelEvent::ToolCall(ToolCall::new("e1", "exclusive")),
            ModelEvent::ToolCall(ToolCall::new("p3", "parallel")),
            ModelEvent::Completed(TokenUsage::default()),
        ],
        text_stream("after tools"),
    ]);
    services.insert_tool(Arc::new(RecordingTool {
        name: "parallel".to_string(),
        mode: ConcurrencyMode::Parallel,
        log: log.clone(),
        delay_ms: 1,
    }));
    services.insert_tool(Arc::new(RecordingTool {
        name: "exclusive".to_string(),
        mode: ConcurrencyMode::Exclusive,
        log: log.clone(),
        delay_ms: 0,
    }));

    let result = run_turn(
        test_context(),
        TurnState::default(),
        &services,
        &praxis_loop::NoopHooks,
        TurnInput::default(),
        CancellationToken::new(),
    )
    .await;

    match result {
        TurnResult::Complete { state } => {
            assert_eq!(state.round_count(), 2);
            assert_eq!(state.tool_call_count(), 4);
        }
        other => panic!("unexpected result: {other:?}"),
    }

    let log = log.lock().expect("log lock").clone();
    let e_start = log
        .iter()
        .position(|entry| entry == "start:e1")
        .expect("exclusive start");
    let p3_start = log
        .iter()
        .position(|entry| entry == "start:p3")
        .expect("p3 start");
    assert!(e_start < p3_start);

    let requests = services.requests();
    assert_eq!(requests.len(), 2);
    assert!(requests[1].prompt.iter().any(|item| matches!(
        item,
        PromptItem::ToolResult { call_id, .. } if call_id == "e1"
    )));
}

struct PolicyHooks;

#[async_trait]
impl TurnHooks for PolicyHooks {
    async fn before_tool_call(&self, view: ToolCallView<'_>) -> ToolDecision {
        if view.call.name == "blocked" {
            return ToolDecision::Block("blocked by policy".to_string());
        }
        ToolDecision::Allow
    }

    async fn after_tool_call(&self, view: ToolResultView<'_>) -> ToolResultDecision {
        match view.call.name.as_str() {
            "rewrite" => {
                ToolResultDecision::Rewrite(ToolResult::success(view.call.id.clone(), "rewritten"))
            }
            "terminate" => {
                ToolResultDecision::Terminate(ToolResult::success(view.call.id.clone(), "stop now"))
            }
            _ => ToolResultDecision::AsIs,
        }
    }
}

#[tokio::test]
async fn hooks_can_block_rewrite_and_terminate_tools() {
    let services = MockServices::with_streams(vec![vec![
        ModelEvent::ToolCall(ToolCall::new("b1", "blocked")),
        ModelEvent::ToolCall(ToolCall::new("r1", "rewrite")),
        ModelEvent::ToolCall(ToolCall::new("t1", "terminate")),
        ModelEvent::Completed(TokenUsage::default()),
    ]]);
    for name in ["rewrite", "terminate"] {
        services.insert_tool(Arc::new(RecordingTool {
            name: name.to_string(),
            mode: ConcurrencyMode::Exclusive,
            log: Arc::new(Mutex::new(Vec::new())),
            delay_ms: 0,
        }));
    }

    let result = run_turn(
        test_context(),
        TurnState::default(),
        &services,
        &PolicyHooks,
        TurnInput::default(),
        CancellationToken::new(),
    )
    .await;

    match result {
        TurnResult::Complete { state } => {
            assert_eq!(state.last_agent_message(), Some("stop now"));
        }
        other => panic!("unexpected result: {other:?}"),
    }

    let persisted = services.persisted();
    assert!(persisted.iter().any(|item| matches!(
        item,
        TurnItem::ToolResult(result)
            if result.call_id == "b1" && result.content == "blocked by policy"
    )));
    assert!(persisted.iter().any(|item| matches!(
        item,
        TurnItem::ToolResult(result)
            if result.call_id == "r1" && result.content == "rewritten"
    )));
    assert!(persisted.iter().any(|item| matches!(
        item,
        TurnItem::ToolResult(result)
            if result.call_id == "t1" && result.content == "stop now"
    )));
}

#[tokio::test]
async fn tool_guard_stops_before_tool_execution() {
    let log = Arc::new(Mutex::new(Vec::new()));
    let services = MockServices::with_streams(vec![vec![
        ModelEvent::ToolCall(ToolCall::new("guarded", "guarded_tool")),
        ModelEvent::Completed(TokenUsage::default()),
    ]]);
    services.insert_tool(Arc::new(RecordingTool {
        name: "guarded_tool".to_string(),
        mode: ConcurrencyMode::Exclusive,
        log: log.clone(),
        delay_ms: 0,
    }));

    let result = run_turn(
        test_context(),
        TurnState::default().with_guard(LoopGuard::with_max_tool_calls(0)),
        &services,
        &praxis_loop::NoopHooks,
        TurnInput::default(),
        CancellationToken::new(),
    )
    .await;

    match result {
        TurnResult::Complete { state } => {
            assert_eq!(state.tool_call_count(), 1);
            assert_eq!(state.last_agent_message(), None);
        }
        other => panic!("unexpected result: {other:?}"),
    }
    assert!(log.lock().expect("log lock").is_empty());
}

#[tokio::test]
async fn cancelled_turn_aborts_before_sampling() {
    let cancel = CancellationToken::new();
    cancel.cancel();
    let services = MockServices::with_streams(vec![text_stream("unused")]);

    let result = run_turn(
        test_context(),
        TurnState::default(),
        &services,
        &praxis_loop::NoopHooks,
        TurnInput::default(),
        cancel,
    )
    .await;

    match result {
        TurnResult::Aborted { reason, .. } => {
            assert_eq!(reason.kind, TurnErrorKind::Cancelled);
        }
        other => panic!("unexpected result: {other:?}"),
    }
}
