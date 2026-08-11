use std::collections::VecDeque;

use futures::FutureExt;
use futures::StreamExt;
use futures::future::BoxFuture;
use futures::stream::FuturesUnordered;
use tokio_util::sync::CancellationToken;

use crate::model::TurnItem;
use crate::outcome::LoopResult;
use crate::services::ToolAccess;
use crate::tool::EffectJournal;
use crate::tool::EffectValidation;
use crate::tool::ToolCall;
use crate::tool::ToolExecutionContext;
use crate::tool::ToolLifecycleSink;
use crate::tool::ToolResult;
use crate::tool::errors::cancelled_tool_result;
use crate::tool::errors::missing_tool_result;
use crate::tool::lifecycle::RecordedToolLifecycle;

use super::plan::PlannedTool;
#[cfg(test)]
use super::plan::ToolExecutionPlan;

pub(crate) struct ToolRun {
    pub(crate) call: ToolCall,
    pub(crate) result: LoopResult<ToolResult>,
    pub(crate) lifecycle_items: Vec<TurnItem>,
    pub(crate) effect_validation: EffectValidation,
}

pub(crate) struct ToolBatchRun {
    pub(crate) entries: Vec<ToolBatchEntry>,
}

pub(crate) enum ToolBatchEntry {
    Immediate(TurnItem),
    Run(ToolRun),
}

enum ProviderPosition {
    Immediate(TurnItem),
    Node(usize),
}

pub(crate) async fn prepare_and_run_tool_calls<A>(
    calls: Vec<ToolCall>,
    access: &A,
    cancel: CancellationToken,
) -> LoopResult<ToolBatchRun>
where
    A: ToolAccess + ToolLifecycleSink + ?Sized,
{
    let mut provider_positions = Vec::new();
    let mut nodes: Vec<Option<PlannedTool>> = Vec::new();
    let mut effects = Vec::new();
    let mut remaining_dependencies = Vec::new();
    let mut dependents: Vec<Vec<usize>> = Vec::new();
    let mut completed = Vec::new();
    let mut ready = VecDeque::new();
    let mut active: FuturesUnordered<BoxFuture<'_, (usize, ToolRun)>> = FuturesUnordered::new();
    let mut ordered: Vec<Option<ToolRun>> = Vec::new();

    for call in calls {
        if cancel.is_cancelled() {
            provider_positions.push(ProviderPosition::Immediate(TurnItem::ToolResult(
                cancelled_tool_result(&call),
            )));
            continue;
        }
        let Some(tool) = access.resolve_tool(&call.name) else {
            provider_positions.push(ProviderPosition::Immediate(TurnItem::ToolResult(
                missing_tool_result(&call),
            )));
            continue;
        };

        start_ready_tools(&mut nodes, &mut ready, &mut active, &cancel, access);

        let prepare_tool = tool.clone();
        let prepare_call = call.clone();
        let mut preparation = Box::pin(async move { prepare_tool.prepare(&prepare_call).await });
        let preparation_result = loop {
            if active.is_empty() {
                break tokio::select! {
                    prepared = &mut preparation => prepared,
                    _ = cancel.cancelled() => Err(crate::outcome::TurnError::cancelled()),
                };
            }
            tokio::select! {
                prepared = &mut preparation => break prepared,
                _ = cancel.cancelled() => break Err(crate::outcome::TurnError::cancelled()),
                Some((index, run)) = active.next() => {
                    complete_run(
                        index,
                        run,
                        &mut ordered,
                        &mut completed,
                        &mut remaining_dependencies,
                        &dependents,
                        &mut ready,
                    );
                    start_ready_tools(
                        &mut nodes,
                        &mut ready,
                        &mut active,
                        &cancel,
                        access,
                    );
                }
            }
        };
        let prepared = match preparation_result {
            Ok(prepared) => prepared,
            Err(error) => {
                provider_positions.push(ProviderPosition::Immediate(TurnItem::ToolResult(
                    ToolResult::error(call.id, error.to_string()),
                )));
                continue;
            }
        };

        let node_effects = prepared.effects().clone();
        let dependencies = effects
            .iter()
            .enumerate()
            .filter_map(|(index, prior_effects)| {
                (!completed[index] && node_effects.conflicts(prior_effects)).then_some(index)
            })
            .collect::<Vec<_>>();
        let index = nodes.len();
        provider_positions.push(ProviderPosition::Node(index));
        for dependency in &dependencies {
            dependents[*dependency].push(index);
        }
        if dependencies.is_empty() {
            ready.push_back(index);
        }
        effects.push(node_effects.clone());
        remaining_dependencies.push(dependencies.len());
        dependents.push(Vec::new());
        completed.push(false);
        ordered.push(None);
        nodes.push(Some(PlannedTool {
            call,
            tool,
            #[cfg(test)]
            dependencies,
            effects: node_effects,
            prepared,
        }));
    }

    loop {
        start_ready_tools(&mut nodes, &mut ready, &mut active, &cancel, access);
        let Some((index, run)) = active.next().await else {
            break;
        };
        complete_run(
            index,
            run,
            &mut ordered,
            &mut completed,
            &mut remaining_dependencies,
            &dependents,
            &mut ready,
        );
    }

    for (index, pending) in nodes.into_iter().enumerate() {
        if let Some(pending) = pending {
            ordered[index] = Some(cancelled_run(pending));
        }
    }

    let mut ordered = ordered;
    let entries = provider_positions
        .into_iter()
        .map(|position| match position {
            ProviderPosition::Immediate(item) => ToolBatchEntry::Immediate(item),
            ProviderPosition::Node(index) => ToolBatchEntry::Run(
                ordered[index]
                    .take()
                    .expect("every planned tool must have a terminal run"),
            ),
        })
        .collect();
    Ok(ToolBatchRun { entries })
}

fn start_ready_tools<'a, P>(
    nodes: &mut [Option<PlannedTool>],
    ready: &mut VecDeque<usize>,
    active: &mut FuturesUnordered<BoxFuture<'a, (usize, ToolRun)>>,
    cancel: &CancellationToken,
    progress: &'a P,
) where
    P: ToolLifecycleSink + ?Sized,
{
    while !cancel.is_cancelled()
        && let Some(index) = ready.pop_front()
    {
        let pending = nodes[index].take().expect("ready tool must be pending");
        let child_cancel = cancel.child_token();
        active.push(async move { (index, run_one(pending, child_cancel, progress).await) }.boxed());
    }
}

fn complete_run(
    index: usize,
    run: ToolRun,
    ordered: &mut [Option<ToolRun>],
    completed: &mut [bool],
    remaining_dependencies: &mut [usize],
    dependents: &[Vec<usize>],
    ready: &mut VecDeque<usize>,
) {
    ordered[index] = Some(run);
    completed[index] = true;
    for dependent in &dependents[index] {
        remaining_dependencies[*dependent] -= 1;
        if remaining_dependencies[*dependent] == 0 {
            ready.push_back(*dependent);
        }
    }
}

#[cfg(test)]
pub(crate) async fn run_tool_plan<P>(
    plan: ToolExecutionPlan,
    cancel: CancellationToken,
    progress: &P,
) -> Vec<ToolRun>
where
    P: ToolLifecycleSink + ?Sized,
{
    let node_count = plan.nodes.len();
    let mut nodes: Vec<Option<PlannedTool>> = plan.nodes.into_iter().map(Some).collect();
    let mut remaining_dependencies = vec![0usize; node_count];
    let mut dependents = vec![Vec::new(); node_count];
    let mut ready = VecDeque::new();

    for (index, node) in nodes.iter().enumerate() {
        let dependencies = &node.as_ref().expect("planned tool").dependencies;
        remaining_dependencies[index] = dependencies.len();
        if dependencies.is_empty() {
            ready.push_back(index);
        }
        for dependency in dependencies {
            dependents[*dependency].push(index);
        }
    }

    let mut active: FuturesUnordered<BoxFuture<'_, (usize, ToolRun)>> = FuturesUnordered::new();
    let mut ordered: Vec<Option<ToolRun>> =
        std::iter::repeat_with(|| None).take(node_count).collect();

    loop {
        while !cancel.is_cancelled()
            && let Some(index) = ready.pop_front()
        {
            let pending = nodes[index].take().expect("ready tool must be pending");
            let child_cancel = cancel.child_token();
            active.push(
                async move { (index, run_one(pending, child_cancel, progress).await) }.boxed(),
            );
        }

        let Some((index, run)) = active.next().await else {
            break;
        };
        ordered[index] = Some(run);
        for dependent in &dependents[index] {
            remaining_dependencies[*dependent] -= 1;
            if remaining_dependencies[*dependent] == 0 {
                ready.push_back(*dependent);
            }
        }
    }

    for (index, pending) in nodes.into_iter().enumerate() {
        if let Some(pending) = pending {
            ordered[index] = Some(cancelled_run(pending));
        }
    }

    ordered.into_iter().flatten().collect()
}

fn cancelled_run(pending: PlannedTool) -> ToolRun {
    let validation = EffectJournal::default().validate(&pending.effects);
    ToolRun {
        result: Ok(cancelled_tool_result(&pending.call)),
        call: pending.call,
        lifecycle_items: Vec::new(),
        effect_validation: validation,
    }
}

async fn run_one<P>(pending: PlannedTool, cancel: CancellationToken, progress: &P) -> ToolRun
where
    P: ToolLifecycleSink + ?Sized,
{
    let journal = EffectJournal::default();
    let planned_effects = pending.effects.clone();
    let validation = || journal.validate(&planned_effects);
    let call = pending.call;
    let prepared = pending.prepared;
    if cancel.is_cancelled() {
        return ToolRun {
            call: call.clone(),
            result: Ok(cancelled_tool_result(&call)),
            lifecycle_items: Vec::new(),
            effect_validation: validation(),
        };
    }
    let (lifecycle, lifecycle_drain) = RecordedToolLifecycle::new(progress);
    if let Err(reason) = lifecycle.tool_started(&call).await {
        drop(lifecycle);
        return ToolRun {
            call,
            result: Err(reason),
            lifecycle_items: lifecycle_drain.finish(),
            effect_validation: validation(),
        };
    }
    let result = pending
        .tool
        .execute_prepared_streaming(
            call.clone(),
            prepared,
            ToolExecutionContext::new(cancel, journal.clone()),
            &lifecycle,
        )
        .await;
    let mut result = result.or_else(|error| {
        Ok::<_, crate::outcome::TurnError>(ToolResult::error(call.id.clone(), error.to_string()))
    });
    if let Ok(completed) = &result
        && let Err(error) = lifecycle.tool_execution_completed(completed).await
    {
        result = Err(error);
    }
    drop(lifecycle);
    ToolRun {
        call,
        result,
        lifecycle_items: lifecycle_drain.finish(),
        effect_validation: validation(),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Arc;
    use std::sync::Mutex;
    use std::sync::atomic::AtomicUsize;
    use std::sync::atomic::Ordering;
    use std::time::Duration;

    use async_trait::async_trait;
    use tokio::sync::Notify;

    use crate::outcome::LoopResult;
    use crate::outcome::TurnError;
    use crate::outcome::TurnErrorKind;
    use crate::services::ToolAccess;
    use crate::tool::EffectKey;
    use crate::tool::Tool;
    use crate::tool::ToolEffect;
    use crate::tool::ToolEffects;
    use crate::tool::ToolExecutionContext;
    use crate::tool::ToolProgress;
    use crate::tool::ToolSpec;

    use super::*;

    struct NoopLifecycle;

    #[async_trait]
    impl ToolLifecycleSink for NoopLifecycle {
        async fn tool_started(&self, _call: &ToolCall) -> LoopResult<()> {
            Ok(())
        }

        async fn tool_progress(&self, _progress: ToolProgress) -> LoopResult<()> {
            Ok(())
        }
    }

    struct TestTool {
        name: &'static str,
        delay: Duration,
        outcome: TestOutcome,
        log: Arc<Mutex<Vec<String>>>,
        started: Option<Arc<Notify>>,
        observed: Option<ToolEffect>,
    }

    struct PipelineTool {
        name: &'static str,
        prepare_wait: Option<Arc<Notify>>,
        execute_notify: Option<Arc<Notify>>,
    }

    #[async_trait]
    impl Tool for PipelineTool {
        fn spec(&self) -> ToolSpec {
            ToolSpec {
                name: self.name.to_string(),
                description: String::new(),
            }
        }

        async fn prepare(&self, _call: &ToolCall) -> LoopResult<crate::tool::PreparedToolCall> {
            if let Some(wait) = &self.prepare_wait {
                wait.notified().await;
            }
            Ok(crate::tool::PreparedToolCall::new(ToolEffects::pure()))
        }

        async fn execute(
            &self,
            call: ToolCall,
            _context: ToolExecutionContext,
        ) -> LoopResult<ToolResult> {
            if let Some(notify) = &self.execute_notify {
                notify.notify_one();
            }
            Ok(ToolResult::success(call.id, self.name))
        }
    }

    struct PipelineRuntime {
        tools: HashMap<String, Arc<dyn Tool>>,
    }

    struct FailingLifecycleRuntime {
        tool: Arc<dyn Tool>,
    }

    struct PreparedEcho {
        preparations: Arc<AtomicUsize>,
    }

    struct PrepareFailureTool;

    struct NeverPrepareTool {
        started: Arc<Notify>,
    }

    #[async_trait]
    impl Tool for NeverPrepareTool {
        fn spec(&self) -> ToolSpec {
            ToolSpec {
                name: "never_prepare".to_string(),
                description: String::new(),
            }
        }

        async fn prepare(&self, _call: &ToolCall) -> LoopResult<crate::tool::PreparedToolCall> {
            self.started.notify_one();
            std::future::pending().await
        }

        async fn execute(
            &self,
            call: ToolCall,
            _context: ToolExecutionContext,
        ) -> LoopResult<ToolResult> {
            Ok(ToolResult::error(call.id, "must not execute"))
        }
    }

    #[async_trait]
    impl Tool for PrepareFailureTool {
        fn spec(&self) -> ToolSpec {
            ToolSpec {
                name: "prepare_failure".to_string(),
                description: String::new(),
            }
        }

        async fn prepare(&self, _call: &ToolCall) -> LoopResult<crate::tool::PreparedToolCall> {
            Err(TurnError::new(TurnErrorKind::Tool, "preparation failed"))
        }

        async fn execute(
            &self,
            call: ToolCall,
            _context: ToolExecutionContext,
        ) -> LoopResult<ToolResult> {
            Ok(ToolResult::error(call.id, "must not execute"))
        }
    }

    #[async_trait]
    impl Tool for PreparedEcho {
        fn spec(&self) -> ToolSpec {
            ToolSpec {
                name: "prepared_echo".to_string(),
                description: String::new(),
            }
        }

        async fn prepare(&self, call: &ToolCall) -> LoopResult<crate::tool::PreparedToolCall> {
            self.preparations.fetch_add(1, Ordering::SeqCst);
            Ok(crate::tool::PreparedToolCall::new(ToolEffects::pure())
                .with_state(call.arguments.clone()))
        }

        async fn execute(
            &self,
            call: ToolCall,
            _context: ToolExecutionContext,
        ) -> LoopResult<ToolResult> {
            Ok(ToolResult::error(call.id, "prepared path was bypassed"))
        }

        async fn execute_prepared_streaming(
            &self,
            call: ToolCall,
            mut prepared: crate::tool::PreparedToolCall,
            _context: ToolExecutionContext,
            _lifecycle: &(dyn ToolLifecycleSink + Send + Sync),
        ) -> LoopResult<ToolResult> {
            let state = prepared
                .take_state::<String>()
                .expect("typed prepared state");
            Ok(ToolResult::success(call.id, state))
        }
    }

    impl ToolAccess for PipelineRuntime {
        fn resolve_tool(&self, name: &str) -> Option<Arc<dyn Tool>> {
            self.tools.get(name).cloned()
        }
    }

    impl ToolAccess for FailingLifecycleRuntime {
        fn resolve_tool(&self, _name: &str) -> Option<Arc<dyn Tool>> {
            Some(Arc::clone(&self.tool))
        }
    }

    #[async_trait]
    impl ToolLifecycleSink for PipelineRuntime {
        async fn tool_started(&self, _call: &ToolCall) -> LoopResult<()> {
            Ok(())
        }

        async fn tool_progress(&self, _progress: ToolProgress) -> LoopResult<()> {
            Ok(())
        }
    }

    #[async_trait]
    impl ToolLifecycleSink for FailingLifecycleRuntime {
        async fn tool_started(&self, _call: &ToolCall) -> LoopResult<()> {
            Err(TurnError::new(
                TurnErrorKind::Internal,
                "lifecycle sink failed",
            ))
        }

        async fn tool_progress(&self, _progress: ToolProgress) -> LoopResult<()> {
            Ok(())
        }
    }

    enum TestOutcome {
        Success,
        Fail,
        WaitForCancellation,
    }

    #[async_trait]
    impl Tool for TestTool {
        fn spec(&self) -> ToolSpec {
            ToolSpec {
                name: self.name.to_string(),
                description: String::new(),
            }
        }

        async fn execute(
            &self,
            call: ToolCall,
            context: ToolExecutionContext,
        ) -> LoopResult<ToolResult> {
            self.log
                .lock()
                .expect("test log lock")
                .push(format!("start:{}", call.id));
            if let Some(effect) = &self.observed {
                context.effects.record(effect.clone());
            }
            if let Some(started) = &self.started {
                started.notify_one();
            }
            match self.outcome {
                TestOutcome::WaitForCancellation => context.cancel.cancelled().await,
                _ => tokio::time::sleep(self.delay).await,
            }
            self.log
                .lock()
                .expect("test log lock")
                .push(format!("finish:{}", call.id));
            match self.outcome {
                TestOutcome::Fail => Err(TurnError::new(TurnErrorKind::Tool, "boom")),
                TestOutcome::Success | TestOutcome::WaitForCancellation => {
                    Ok(ToolResult::success(call.id, self.name))
                }
            }
        }
    }

    fn file(path: &str) -> EffectKey {
        EffectKey::hierarchical("filesystem", path.split('/'))
    }

    fn node(
        id: &str,
        tool: Arc<dyn Tool>,
        dependencies: Vec<usize>,
        effects: ToolEffects,
    ) -> PlannedTool {
        PlannedTool {
            call: ToolCall::new(id, tool.spec().name),
            tool,
            dependencies,
            prepared: crate::tool::PreparedToolCall::new(effects.clone()),
            effects,
        }
    }

    #[tokio::test]
    async fn concurrent_completion_is_drained_in_provider_order() {
        let log = Arc::new(Mutex::new(Vec::new()));
        let first = Arc::new(TestTool {
            name: "first",
            delay: Duration::from_millis(25),
            outcome: TestOutcome::Success,
            log: Arc::clone(&log),
            started: None,
            observed: None,
        });
        let second = Arc::new(TestTool {
            name: "second",
            delay: Duration::ZERO,
            outcome: TestOutcome::Success,
            log: Arc::clone(&log),
            started: None,
            observed: None,
        });
        let runs = run_tool_plan(
            ToolExecutionPlan {
                nodes: vec![
                    node("first", first, Vec::new(), ToolEffects::pure()),
                    node("second", second, Vec::new(), ToolEffects::pure()),
                ],
            },
            CancellationToken::new(),
            &NoopLifecycle,
        )
        .await;

        assert_eq!(
            runs.iter()
                .map(|run| run.call.id.as_str())
                .collect::<Vec<_>>(),
            vec!["first", "second"]
        );
        let log = log.lock().expect("test log lock");
        assert!(
            log.iter().position(|entry| entry == "finish:second")
                < log.iter().position(|entry| entry == "finish:first")
        );
    }

    #[tokio::test]
    async fn failed_tool_releases_its_dependents() {
        let log = Arc::new(Mutex::new(Vec::new()));
        let failing = Arc::new(TestTool {
            name: "failing",
            delay: Duration::ZERO,
            outcome: TestOutcome::Fail,
            log: Arc::clone(&log),
            started: None,
            observed: None,
        });
        let dependent = Arc::new(TestTool {
            name: "dependent",
            delay: Duration::ZERO,
            outcome: TestOutcome::Success,
            log: Arc::clone(&log),
            started: None,
            observed: None,
        });
        let shared = ToolEffects::write(file("repo/a.rs"));
        let runs = run_tool_plan(
            ToolExecutionPlan {
                nodes: vec![
                    node("failing", failing, Vec::new(), shared.clone()),
                    node("dependent", dependent, vec![0], shared),
                ],
            },
            CancellationToken::new(),
            &NoopLifecycle,
        )
        .await;

        assert_eq!(runs.len(), 2);
        assert!(runs[0].result.as_ref().expect("failure result").is_error());
        assert!(
            log.lock()
                .expect("test log lock")
                .iter()
                .any(|entry| entry == "start:dependent")
        );
    }

    #[tokio::test]
    async fn cancellation_settles_queued_tools_without_starting_them() {
        let log = Arc::new(Mutex::new(Vec::new()));
        let started = Arc::new(Notify::new());
        let active = Arc::new(TestTool {
            name: "active",
            delay: Duration::ZERO,
            outcome: TestOutcome::WaitForCancellation,
            log: Arc::clone(&log),
            started: Some(Arc::clone(&started)),
            observed: None,
        });
        let queued = Arc::new(TestTool {
            name: "queued",
            delay: Duration::ZERO,
            outcome: TestOutcome::Success,
            log: Arc::clone(&log),
            started: None,
            observed: None,
        });
        let shared = ToolEffects::write(file("repo/a.rs"));
        let cancel = CancellationToken::new();
        let task_cancel = cancel.clone();
        let task = tokio::spawn(async move {
            run_tool_plan(
                ToolExecutionPlan {
                    nodes: vec![
                        node("active", active, Vec::new(), shared.clone()),
                        node("queued", queued, vec![0], shared),
                    ],
                },
                task_cancel,
                &NoopLifecycle,
            )
            .await
        });
        started.notified().await;
        cancel.cancel();
        let runs = task.await.expect("tool plan task");

        assert_eq!(runs.len(), 2);
        assert_eq!(runs[0].call.id, "active");
        assert_eq!(runs[1].call.id, "queued");
        assert!(runs[1].result.as_ref().expect("queued result").is_error());
        assert_eq!(
            log.lock().expect("test log lock").as_slice(),
            ["start:active", "finish:active"]
        );
    }

    #[tokio::test]
    async fn runtime_effects_outside_the_plan_are_reported() {
        let tool = Arc::new(TestTool {
            name: "observer",
            delay: Duration::ZERO,
            outcome: TestOutcome::Success,
            log: Arc::new(Mutex::new(Vec::new())),
            started: None,
            observed: Some(ToolEffect::write(file("repo/other.rs"))),
        });
        let runs = run_tool_plan(
            ToolExecutionPlan {
                nodes: vec![node(
                    "observer",
                    tool,
                    Vec::new(),
                    ToolEffects::write(file("repo/planned.rs")),
                )],
            },
            CancellationToken::new(),
            &NoopLifecycle,
        )
        .await;

        assert!(!runs[0].effect_validation.is_valid());
        assert_eq!(runs[0].effect_validation.unexpected.len(), 1);
    }

    #[tokio::test]
    async fn later_independent_tool_starts_while_an_earlier_conflict_is_queued() {
        let log = Arc::new(Mutex::new(Vec::new()));
        let first = Arc::new(TestTool {
            name: "first",
            delay: Duration::from_millis(25),
            outcome: TestOutcome::Success,
            log: Arc::clone(&log),
            started: None,
            observed: None,
        });
        let queued = Arc::new(TestTool {
            name: "queued",
            delay: Duration::ZERO,
            outcome: TestOutcome::Success,
            log: Arc::clone(&log),
            started: None,
            observed: None,
        });
        let independent = Arc::new(TestTool {
            name: "independent",
            delay: Duration::ZERO,
            outcome: TestOutcome::Success,
            log: Arc::clone(&log),
            started: None,
            observed: None,
        });
        let shared = ToolEffects::write(file("repo/a.rs"));
        let runs = run_tool_plan(
            ToolExecutionPlan {
                nodes: vec![
                    node("first", first, Vec::new(), shared.clone()),
                    node("queued", queued, vec![0], shared),
                    node(
                        "independent",
                        independent,
                        Vec::new(),
                        ToolEffects::read(file("repo/b.rs")),
                    ),
                ],
            },
            CancellationToken::new(),
            &NoopLifecycle,
        )
        .await;

        assert_eq!(runs.len(), 3);
        let log = log.lock().expect("test log lock");
        assert!(
            log.iter().position(|entry| entry == "start:independent")
                < log.iter().position(|entry| entry == "start:queued")
        );
    }

    #[tokio::test]
    async fn exclusive_tool_is_a_provider_order_barrier() {
        let log = Arc::new(Mutex::new(Vec::new()));
        let make = |name: &'static str| {
            Arc::new(TestTool {
                name,
                delay: Duration::ZERO,
                outcome: TestOutcome::Success,
                log: Arc::clone(&log),
                started: None,
                observed: None,
            }) as Arc<dyn Tool>
        };
        let runs = run_tool_plan(
            ToolExecutionPlan {
                nodes: vec![
                    node(
                        "first",
                        make("first"),
                        Vec::new(),
                        ToolEffects::read(file("repo/a.rs")),
                    ),
                    node(
                        "exclusive",
                        make("exclusive"),
                        vec![0],
                        ToolEffects::unknown_write(),
                    ),
                    node(
                        "last",
                        make("last"),
                        vec![1],
                        ToolEffects::read(file("repo/b.rs")),
                    ),
                ],
            },
            CancellationToken::new(),
            &NoopLifecycle,
        )
        .await;

        assert_eq!(runs.len(), 3);
        let starts = log
            .lock()
            .expect("test log lock")
            .iter()
            .filter(|entry| entry.starts_with("start:"))
            .cloned()
            .collect::<Vec<_>>();
        assert_eq!(starts, ["start:first", "start:exclusive", "start:last"]);
    }

    #[tokio::test]
    async fn execution_overlaps_preparation_of_later_provider_calls() {
        let first_started = Arc::new(Notify::new());
        let runtime = PipelineRuntime {
            tools: HashMap::from([
                (
                    "first".to_string(),
                    Arc::new(PipelineTool {
                        name: "first",
                        prepare_wait: None,
                        execute_notify: Some(Arc::clone(&first_started)),
                    }) as Arc<dyn Tool>,
                ),
                (
                    "second".to_string(),
                    Arc::new(PipelineTool {
                        name: "second",
                        prepare_wait: Some(first_started),
                        execute_notify: None,
                    }) as Arc<dyn Tool>,
                ),
            ]),
        };

        let batch = tokio::time::timeout(
            Duration::from_millis(250),
            prepare_and_run_tool_calls(
                vec![
                    ToolCall::new("one", "first"),
                    ToolCall::new("two", "second"),
                ],
                &runtime,
                CancellationToken::new(),
            ),
        )
        .await
        .expect("first execution must unblock second preparation")
        .expect("pipeline run");

        assert_eq!(
            batch
                .entries
                .iter()
                .filter_map(|entry| match entry {
                    ToolBatchEntry::Run(run) => Some(run.call.id.as_str()),
                    ToolBatchEntry::Immediate(_) => None,
                })
                .collect::<Vec<_>>(),
            ["one", "two"]
        );
    }

    #[tokio::test]
    async fn missing_tool_results_keep_provider_order_between_executed_calls() {
        let runtime = PipelineRuntime {
            tools: HashMap::from([(
                "echo".to_string(),
                Arc::new(PipelineTool {
                    name: "echo",
                    prepare_wait: None,
                    execute_notify: None,
                }) as Arc<dyn Tool>,
            )]),
        };
        let batch = prepare_and_run_tool_calls(
            vec![
                ToolCall::new("first", "echo"),
                ToolCall::new("missing", "not_registered"),
                ToolCall::new("last", "echo"),
            ],
            &runtime,
            CancellationToken::new(),
        )
        .await
        .expect("pipeline run");

        let ids = batch
            .entries
            .iter()
            .map(|entry| match entry {
                ToolBatchEntry::Run(run) => run.call.id.as_str(),
                ToolBatchEntry::Immediate(TurnItem::ToolResult(result)) => result.call_id.as_str(),
                ToolBatchEntry::Immediate(_) => panic!("unexpected immediate item"),
            })
            .collect::<Vec<_>>();
        assert_eq!(ids, ["first", "missing", "last"]);
    }

    #[tokio::test]
    async fn duplicate_call_ids_cannot_cross_wire_prepared_state() {
        let preparations = Arc::new(AtomicUsize::new(0));
        let runtime = PipelineRuntime {
            tools: HashMap::from([(
                "prepared_echo".to_string(),
                Arc::new(PreparedEcho {
                    preparations: Arc::clone(&preparations),
                }) as Arc<dyn Tool>,
            )]),
        };
        let mut first = ToolCall::new("duplicate", "prepared_echo");
        first.arguments = "first".to_string();
        let mut second = ToolCall::new("duplicate", "prepared_echo");
        second.arguments = "second".to_string();

        let batch =
            prepare_and_run_tool_calls(vec![first, second], &runtime, CancellationToken::new())
                .await
                .expect("pipeline run");
        let contents = batch
            .entries
            .into_iter()
            .map(|entry| match entry {
                ToolBatchEntry::Run(run) => run.result.expect("tool result").content,
                ToolBatchEntry::Immediate(_) => panic!("unexpected immediate item"),
            })
            .collect::<Vec<_>>();

        assert_eq!(preparations.load(Ordering::SeqCst), 2);
        assert_eq!(contents, ["first", "second"]);
    }

    #[tokio::test]
    async fn preparation_failure_is_terminal_without_abandoning_other_calls() {
        let runtime = PipelineRuntime {
            tools: HashMap::from([
                (
                    "echo".to_string(),
                    Arc::new(PipelineTool {
                        name: "echo",
                        prepare_wait: None,
                        execute_notify: None,
                    }) as Arc<dyn Tool>,
                ),
                (
                    "prepare_failure".to_string(),
                    Arc::new(PrepareFailureTool) as Arc<dyn Tool>,
                ),
            ]),
        };
        let batch = prepare_and_run_tool_calls(
            vec![
                ToolCall::new("first", "echo"),
                ToolCall::new("failed", "prepare_failure"),
                ToolCall::new("last", "echo"),
            ],
            &runtime,
            CancellationToken::new(),
        )
        .await
        .expect("pipeline run");

        let ids = batch
            .entries
            .iter()
            .map(|entry| match entry {
                ToolBatchEntry::Run(run) => run.call.id.as_str(),
                ToolBatchEntry::Immediate(TurnItem::ToolResult(result)) => result.call_id.as_str(),
                ToolBatchEntry::Immediate(_) => panic!("unexpected immediate item"),
            })
            .collect::<Vec<_>>();
        assert_eq!(ids, ["first", "failed", "last"]);
        assert!(matches!(
            &batch.entries[1],
            ToolBatchEntry::Immediate(TurnItem::ToolResult(result)) if result.is_error()
        ));
    }

    #[tokio::test]
    async fn cancellation_interrupts_a_stalled_preparation() {
        let started = Arc::new(Notify::new());
        let runtime = PipelineRuntime {
            tools: HashMap::from([(
                "never_prepare".to_string(),
                Arc::new(NeverPrepareTool {
                    started: Arc::clone(&started),
                }) as Arc<dyn Tool>,
            )]),
        };
        let cancel = CancellationToken::new();
        let pipeline = prepare_and_run_tool_calls(
            vec![ToolCall::new("stalled", "never_prepare")],
            &runtime,
            cancel.clone(),
        );
        tokio::pin!(pipeline);

        tokio::select! {
            _ = started.notified() => cancel.cancel(),
            _ = &mut pipeline => panic!("pipeline completed before cancellation"),
        }
        let batch = tokio::time::timeout(Duration::from_millis(250), pipeline)
            .await
            .expect("cancellation must interrupt preparation")
            .expect("pipeline run");
        assert!(matches!(
            &batch.entries[0],
            ToolBatchEntry::Immediate(TurnItem::ToolResult(result)) if result.is_error()
        ));
    }

    #[tokio::test]
    async fn lifecycle_start_failure_is_terminal_and_never_executes_the_tool() {
        let log = Arc::new(Mutex::new(Vec::new()));
        let runtime = FailingLifecycleRuntime {
            tool: Arc::new(TestTool {
                name: "blocked_by_lifecycle",
                delay: Duration::ZERO,
                outcome: TestOutcome::Success,
                log: Arc::clone(&log),
                started: None,
                observed: None,
            }),
        };
        let batch = prepare_and_run_tool_calls(
            vec![ToolCall::new("blocked", "blocked_by_lifecycle")],
            &runtime,
            CancellationToken::new(),
        )
        .await
        .expect("pipeline run");

        assert!(matches!(
            &batch.entries[0],
            ToolBatchEntry::Run(run) if run.result.is_err()
        ));
        assert!(log.lock().expect("test log lock").is_empty());
    }
}
