# Praxis Loop Cleanroom — Turn Orchestration Crate

Date: 2026-06-18

This document is the detailed execution plan for Cleanroom Battle #2
("Agent Turn Loop clarification") from
[`cleanroom_target_architecture.md`](./cleanroom_target_architecture.md).

It specifies how `agent_turn_loop` becomes an independent
`praxis-loop` crate: a generic turn orchestration library. The two upper
loops (`main_agent_loop`, `agent_task_loop`) and all concrete runtimes stay in
`praxis-core`. Only the turn loop is extracted.

## 1. Goal and Principles

Extract the turn-level model/tool loop out of `praxis-core` into a standalone
`praxis-loop` crate that depends on neither `praxis-core` nor
`praxis-protocol`. The loop becomes pure orchestration; behavior is injected
through hooks; tools declare their own concurrency.

Three first principles:

| # | Principle | Rejects |
|---|---|---|
| P1 | State flows through the loop, it does not live on a god object. | `run_turn(&self)` |
| P2 | Behavior is a trait hook, not a hardcoded line in the loop body. | `run_before_model_request_compact()` welded inline |
| P3 | Concurrency is a tool property, not a loop property. | one `FuturesOrdered` for everything |

### ECS framing

The skeleton is a hand-written mini ECS, and that framing is deliberate:

| ECS concept | praxis-loop element |
|---|---|
| Component | `TurnState` (turn-local data owned by the loop) |
| Resource | `TurnServices` (five traits; capabilities borrowed, not owned) |
| System | `TurnHooks` (declares data needs, returns decisions) |
| Schedule | `turn_loop.rs` (orchestrates execution order) |

We borrow the ECS **shape** (Query-style minimal access, Plugin-style
composition) but we do not adopt the Bevy frame scheduler: a turn loop is an
async, event-driven stream (model tokens, tool completion, steering), not a
fixed-rate frame loop. `run_turn` stays a hand-written async function.

## 2. Implementation Constraints

Eight constraints prevent the skeleton from bloating or leaking dependencies.

Boundary constraints (prevent dependency leakage):

- **C1** — `praxis-loop` does not depend on `praxis-protocol`. It defines its
  own minimal `TurnId` / `ThreadId` / `ModelEvent` / `ToolCall`. The Praxis
  adapter converts at the edge.
- **C2** — `praxis-loop` owns only the turn loop. `main_agent_loop` and
  `agent_task_loop` stay in `praxis-core` and call `praxis_loop::run_turn`.
- **C3** — `ToolRegistry` is an abstract trait boundary. Concrete tools
  (shell, MCP, apply_patch, sandbox) stay in `praxis-core::tools`.

Internal constraints (prevent interface bloat):

- **C4** — `TurnServices` is split into five small traits combined as an
  alias: `ModelService + EventSink + HistorySink + SteeringInbox + ToolAccess`.
- **C5** — Every hook returns a strongly typed decision enum. No
  `anyhow::Result<Option<...>>` mixing semantics.
- **C6** — `TurnState` stores only turn-local state. It must not hold session
  config, provider client, tool registry, or event sink (those are borrowed
  via services).
- **C7** — The loop file is `turn_loop.rs` (`loop` is a reserved word); the
  public function is still `run_turn`.
- **C8** — Hook inputs are Query-minimized: a hook signature takes only the
  field slice it needs, never a full `&mut TurnState`.

## 3. Three-State Separation

This dissolves the `Arc<Session>` god object that the current turn loop
requires.

| State | Type | Nature |
|---|---|---|
| read | `TurnContext` | frozen at turn start, immutable `Clone` value |
| write | `TurnState` | turn-local, owned exclusively by the loop, returned on exit |
| borrow | `TurnServices` | five traits, external capabilities injected |

`TurnState` returned on turn exit lets the caller decide whether to commit,
which makes fork/resume natural: a turn's intermediate state can be snapshotted
or discarded without being welded to `Session`.

## 4. Crate Boundary

```text
praxis-loop            generic turn orchestration crate
  owns:    run_turn, turn-local state, hook decision contracts, abstract tool dispatch
  must not know: Session, task/submission, rollout impl, TUI/app-gateway,
                 provider concrete clients, shell/MCP/sandbox, praxis-protocol

praxis-core::praxis::agent_turn_loop    Praxis adapter/facade
  implements the five services + PraxisDefaults (TurnHooks)
  converts protocol <-> loop types
  delegates to PraxisTurnLoopBridge, which calls praxis_loop::run_turn

praxis-core::tools      concrete tool runtime
  implements praxis_loop::Tool + declares ConcurrencyMode
  shell / MCP / apply_patch / sandbox all live here

praxis-core::praxis::agent_task_loop    repeated-turn task control (stays in core)
praxis-core::praxis::main_agent_loop    submission/op dispatch (stays in core)
```

This matches the ownership table in section 4 of the target architecture:
`agent_turn_loop` owns one model/tool turn; everything above and below stays
in its own layer.

## 5. File Structure

```text
praxis-rs/loop/                          crate: praxis-loop
└── src/
    ├── lib.rs                           public facade + run_turn re-export
    │
    ├── context.rs                       TurnContext (immutable read-only snapshot)
    ├── state.rs                         TurnState (pure turn-local) + TokenLedger + TurnDiffTracker
    ├── guard.rs                         LoopGuard
    │
    ├── ids.rs                           self-defined TurnId/ThreadId/TraceId          C1
    ├── model.rs                         minimal ModelSpec/ModelEvent/TokenUsage        C1
    │
    ├── services/                        five small traits                              C4
    │   ├── mod.rs                       TurnServices = combined alias
    │   ├── model.rs                     ModelService trait
    │   ├── event.rs                     EventSink trait
    │   ├── history.rs                   HistorySink trait
    │   ├── steering.rs                  SteeringInbox trait
    │   └── tool.rs                      ToolAccess trait (consumes ToolRegistry)
    │
    ├── hooks.rs                         TurnHooks trait - minimal decision hooks        C8 minimal access
    ├── decisions.rs                     strongly typed *Decision enums                 C5
    ├── noop.rs                          NoopHooks
    ├── compose.rs                       ChainedHooks<A,B>
    │
    ├── turn_loop.rs                     run_turn - pure orchestration                  C7
    ├── stream.rs                        internal consume_model_stream
    │
    ├── tool/
    │   ├── mod.rs                       module boundary and re-exports
    │   ├── traits.rs                    Tool + ToolRegistry + ToolLifecycleSink traits              C3
    │   ├── types.rs                     ConcurrencyMode + ToolCall + ToolResult
    │   ├── batch.rs                     concurrency batch partitioning
    │   ├── prepare.rs                   hook-gated call preparation
    │   ├── lifecycle.rs                 tool lifecycle event forwarding
    │   ├── errors.rs                    tool error conversion
    │   └── dispatch.rs                  internal batch drain + result recording
    │
    └── outcome.rs                       TurnResult + TurnError
```

## 6. Module Sketches

### Self-owned abstractions (C1)

`ids.rs`:

```text
TurnId(String), ThreadId(String), TraceId(String)   // newtypes, not protocol re-exports
```

`model.rs`:

```text
ModelSpec { slug, provider_id, context_window, input_modalities }
ModelEvent = TextDelta(String) | ReasoningDelta(String) | ToolCall(ToolCall)
           | FinalText(String) | Completed(TokenUsage)
PromptItem::ToolResult { call_id, content, status: ToolResultStatus }  // serialized as is_error
TokenUsage { input, output, total, reasoning }
```

### Three-state layer

`context.rs` — immutable input:

```text
TurnContext {
  turn_id, thread_id, trace_id                     // identity
  model: ModelSpec, reasoning, service_tier        // model
  permissions, collaboration_mode, cwd, tools, features   // policy
}  // Clone value, not Arc
```

`state.rs` — pure turn-local (C6):

```text
TurnState {
  transcript_delta: private Vec<TurnItem>,  // turn-local items recorded this turn
  token_usage: private TokenLedger,
  tool_call_count: private u64,
  round_count: private u64,
  last_agent_message: private Option<String>,
  guard: private LoopGuard,
  diff: private TurnDiffTracker,
}  // forbidden: config / client / registry / sink / session_id
TokenLedger { cumulative, turn_delta }
TurnDiffTracker { mark_changed(), has_changes() }
TurnDiffStatus = Clean | Changed          // serialized as has_changes at the state edge
TurnState methods:
  with_guard(LoopGuard) -> TurnState
  transcript_delta() -> &[TurnItem]
  token_usage() -> &TokenLedger
  diff() -> &TurnDiffTracker
  round_count() -> u64
  tool_call_count() -> u64
  start_round() -> u64
  record_tool_calls(count) -> ToolCallAdmission
  record_items(items)                 // empty item sets do not mark diff changed
  record_last_agent_message(text)
  record_completion_message(TurnCompletionMessage)
  last_agent_message() -> Option<&str>
  last_completion_message() -> TurnCompletionMessage
  into_last_agent_message() -> Option<String>
```

`guard.rs`:

```text
LoopGuard { max_tool_calls: ToolCallLimit }
LoopGuard::admit_tool_calls(attempted_tool_call_count) -> ToolCallAdmission
ToolCallLimit = Unlimited | Capped { max_tool_calls }  // serialized as optional max_tool_calls
ToolCallAdmission = Accepted | Rejected { attempted_tool_call_count }
```

### Services layer (C4, ECS Resource)

```text
trait ModelService   { stream_model(prompt, model, opts, cancel) -> Stream<ModelEvent> }
trait EventSink      { emit(TurnEvent) }
trait HistorySink    { persist(items) }
trait SteeringInbox  { drain() -> SteeringDrain { messages, control } }
SteeringControl      = Continue | RetryWithoutModelRequest | StopWithoutModelRequest(TurnCompletionMessage)
trait ToolAccess     { resolve(name) -> Option<Arc<dyn Tool>> }   // consumes ToolRegistry, holds no runtime

TurnServices = ModelService + EventSink + HistorySink + SteeringInbox + ToolAccess
// expressed via blanket impl or generic bound <S: ...>
```

### Decision contracts (C5 strong typing)

`decisions.rs` — one enum per decision point, no mixed semantics:

```text
TurnStartDecision       = Proceed | ReplaceInitialPrompt(Vec<PromptItem>) | Abort(TurnError)
ContextPressureDecision = Proceed
                        | Compacted { prompt_items: Vec<PromptItem>, transcript_items: Vec<TurnItem> }
                        | Abort(TurnError)
PrepareContextDecision  = Prepared(Vec<PromptItem>) | Stop(TurnCompletionMessage) | Abort(TurnError)
SteeringDecision        = InjectAndContinue | DropAndContinue
ToolDecision            = Allow | Block(reason) | Modify(ToolCall)
ToolResultDecision      = AsIs | Rewrite(ToolResult) | Terminate(result)
RoundDecision           = Continue { prompt_update } | Stop(TurnCompletionMessage) | Abort(TurnError)
RoundPromptUpdate       = Reuse | Replace(Vec<PromptItem>)
RoundAdjustment         { model: Option<ModelSpec>, reasoning: Option<...> }
PrepareNextRoundDecision = Reuse | Adjust(RoundAdjustment)
TurnStopDecision        = Complete | ContinueTurn | Abort(TurnError)
PrepareStopFlow         = CompleteTurn | ContinueToRounds | Abort(TurnError)
RoundStopFlow           = BreakRounds | ContinueRounds | Abort(TurnError)
TurnCompletionDecision  = Complete | WantsFollowup
TurnCompletionMessage   = NoMessage | Text(String)
```

### Hooks layer (P2 + C8 minimal access)

`hooks.rs` — narrowed decision hooks:

```text
trait TurnHooks {
  // Phase 1: entry / before model request
  on_turn_start(ctx)                              -> TurnStartDecision
  on_context_pressure(view: ContextPressureView)  -> ContextPressureDecision
  prepare_context(view: PrepareContextView)       -> PrepareContextDecision

  // Phase 2: model interaction
  on_steering_input(view: SteeringInputView)      -> SteeringDecision

  // Phase 3: tool lifecycle
  before_tool_call(view: ToolCallView)            -> ToolDecision
  after_tool_call(view: ToolResultView)           -> ToolResultDecision

  // Phase 4: per-round decision
  after_model_round(view: RoundOutcomeView)       -> RoundDecision
  prepare_next_round(ctx)                         -> PrepareNextRoundDecision

  // Phase 5: exit
  on_turn_stop(view: TurnStopView)                -> TurnStopDecision
  after_turn_complete(ctx)                        -> TurnCompletionDecision
}
// every method has a default; PraxisDefaults preserves current behavior via defaults
```

`noop.rs`:

```text
NoopHooks {}  // implements TurnHooks, every method keeps its default
```

`compose.rs`:

```text
ChainedHooks<A,B>  // A then B; deny wins for approvals, first-rewrite-wins for results
// (the ECS Plugin-composition equivalent)
```

### Loop body (P1/P2, pure orchestration)

`turn_loop.rs` — C7 naming:

```text
fn run_turn(ctx, mut state, services, hooks, input, cancel) -> TurnResult:
  match hooks.on_turn_start(ctx):  Abort(e) -> return Aborted(e)
  loop:
    match hooks.on_context_pressure(state.token_usage(), ctx.model.context_window):
      Abort(e) -> return; Proceed|Compacted -> ()
    state.record_items(hooks.prepare_context(...).prepared_items)
    // Stop from preparation records the completion message and uses normal stop hooks.
    match services.steering.drain().control:
      StopWithoutModelRequest(message) -> run normal stop hooks with message
      RetryWithoutModelRequest -> continue without model request
      Continue -> match hooks.on_steering_input(...)
    stream = services.model.stream_model(prompt::build_prompt(...), ...)
    round = consume_model_stream(stream, hooks, services, state)
    match hooks.after_model_round(&round, state.token_usage()):
      Stop(message) -> state.record_completion_message(message); break
      Continue { prompt_update } -> apply prompt_update; apply hooks.prepare_next_round(ctx); continue
      Abort(e) -> return
  loop: match hooks.on_turn_stop(ctx, state.last_agent_message()):
    Complete -> break; ContinueTurn -> again; Abort -> return
  // Internal stop flow uses context-specific decision enums, not Result<Option<_>> control glue.
  match hooks.after_turn_complete(ctx):
    Complete -> Complete(state)
    WantsFollowup -> WantsFollowup(state)
  // loop body contains zero business nouns
```

`stream.rs`:

```text
fn consume_model_stream(stream, hooks, services, state) -> RoundOutcome:
  while item = stream.next():
    match item:
      TextDelta(t)      -> services.event.emit(...); state.record_items(...)
      ReasoningDelta    -> emit
      ToolCall(c)       -> calls.push(c)
      FinalText(t)      -> final = t
      FollowupRequired  -> followup.require()
      Completed(u)      -> state.record_usage(&u); break
  if !calls.empty():
    tool::dispatch(calls, services.tool, hooks, services.event, cancel)
    return ToolCalls(calls) | TerminatedByTool(message)
  followup.into_round_outcome(final)
```

### Tool layer (P3 + C3)

`tool/types.rs` and `tool/traits.rs`:

```text
enum ConcurrencyMode { Parallel | Exclusive | Blocking }
trait Tool {
  spec() -> ToolSpec
  concurrency() -> ConcurrencyMode        // default Parallel
  execute(call, cancel) -> ToolResult
  execute_streaming(call, cancel, sink)   // default delegates to execute
}
trait ToolRegistry {                      // abstract boundary
  get(name) -> Option<Arc<dyn Tool>>
}
struct ToolCall { id, name, arguments }
struct ToolResult { call_id, content, status: ToolResultStatus }
ToolResultStatus = Success | Error      // serialized as is_error at the wire edge
ToolResult::with_status(call_id, content, ToolResultStatus)
Adapter converts protocol success flags with ToolResultStatus::from_success_flag(...)
trait ToolProgressSink { progress(call_id, partial) }
```

`tool/dispatch.rs`:

```text
fn dispatch(calls, registry, hooks, event, cancel):
  batches = partition(calls, registry)   // Blocking/Exclusive own a batch, Parallel share one
  for batch:
    FuturesOrdered concurrent streaming drain   // preserves Praxis streaming emit
    per call:
      tool = registry.get(call.name)
      result = tool.execute_streaming(call, cancel, event)
      control = record_result_decision(hooks.after_tool_call(call, result))
      match control:
        Continue -> next call
        Terminate -> break all remaining batches
      persist_turn_items(items)   // owns empty-set no-op, history persist, state delta, tool-finished event
```

`tool/mod.rs` exports only the external tool contract:

```text
pub use traits::{Tool, ToolLifecycleSink, ToolRegistry}
pub use types::{ConcurrencyMode, ToolCall, ToolProgress, ToolResult, ToolResultStatus, ToolSpec}
// internal only: batch, dispatch, prepare, lifecycle, errors
```

### Outcome layer

`outcome.rs`:

```text
TurnResult = Complete{state} | WantsFollowup{state} | Aborted{reason}
TurnError(String)                          // not anyhow
RoundOutcome = Empty
             | FinalAnswer { message: TurnCompletionMessage }
             | FollowupRequired
             | ToolCalls { calls: Vec<ToolCall> }
             | TerminatedByTool { message: TurnCompletionMessage }
TurnCompletionMessage = NoMessage | Text(String)
```

## 7. Cargo.toml (reflects C1)

```toml
[package]
name = "praxis-loop"
version.workspace = true
edition.workspace = true
license.workspace = true
[lints]
workspace = true

[dependencies]
tokio = { workspace = true }
tokio-util = { workspace = true }     # CancellationToken
futures = { workspace = true }        # FuturesOrdered / Stream
async-trait = { workspace = true }
serde = { workspace = true }          # only for self-owned minimal types
tracing = { workspace = true }
# no praxis-protocol, no praxis-core, no provider/exec/sandbox crates
```

Workspace registration:

```toml
[workspace]
members = [ ..., "loop" ]
[workspace.dependencies]
praxis-loop = { path = "loop" }
```

## 8. Call Topology

```text
praxis-core::praxis::main_agent_loop       op dispatch (stays in core)
        |
praxis-core::praxis::agent_task_loop       repeated turns (stays in core)
        |
praxis-core::praxis::agent_turn_loop       thin facade (stays in core)
        |
praxis-core::praxis::turn_loop_adapter     Praxis adapter boundary
        |   - protocol <-> loop type conversion
        |   - owns PraxisTurnLoopBridge and PraxisTurnLoopOutcome
        |   - owns provider-stream driver/state/side-effect projection
        |   - implements ModelService / EventSink / HistorySink / SteeringInbox / ToolAccess
        |   - PraxisDefaults: implements TurnHooks
        v
praxis_loop::run_turn(ctx, state, services, hooks)   <- new crate, pure orchestration
        | tool dispatch
ToolAccess trait -> praxis-core::tools      shell / MCP / apply_patch / sandbox
```

## 9. ECS Influence Summary

| ECS concept | praxis-loop implementation | Borrow Bevy runtime? |
|---|---|---|
| Component | TurnState (turn-local data) | no, plain struct |
| Resource | TurnServices five traits (borrowed capabilities) | no, plain traits |
| System | TurnHooks (minimal decision hooks) | no, plain trait |
| Schedule | turn_loop.rs (orchestration) | no - async event-driven, not frame scheduling |
| Query minimal access | C8: hooks take only the field slice they need | shape only, no real Query |
| Plugin composition | ChainedHooks<A,B> | shape only, no Bevy Plugin |

Bevy scheduler is not adopted because a turn loop is an async streaming event
sequence (token stream, tool completion, steering), while Bevy Schedule is a
synchronous frame scheduler; the semantics do not match. `run_turn` stays a
hand-written async function; we borrow ECS shape and data-access discipline only.

## 10. Migration (three phases)

1. **Skeleton** — create `praxis-rs/loop/`, all traits + structs + `NoopHooks`
   + mock-services unit tests. `cargo test -p praxis-loop` passes with zero
   `praxis-core` dependency.
2. **Move behavior into hooks** — `praxis-core` writes `PraxisDefaults:
   TurnHooks` (move compact / prepare / stop-hooks into default impls),
   implements the five services, adds `ConcurrencyMode` to tools. New and old
   loops run shadow comparison to verify equivalence.
3. **Switch** — `agent_turn_loop` delegates to `turn_loop_adapter`;
   `PraxisTurnLoopBridge` calls `praxis_loop::run_turn`. The old
   `turn_prepare.rs` and `turn_stop_hooks.rs` ownership has moved inside the
   adapter as `prepare_phase.rs` and `stop_hooks.rs`; provider-stream driver,
  response dispatch, stream item state, and stream side effects now live under
  `turn_loop_adapter/model_stream`. Provider request assembly has been split:
  shared provider prompt and tool-router construction lives in
   `praxis::model_request`, realtime event mirroring lives in
   `event_text_projection`, and compact summary text extraction lives in
   `turn_assistant_text`. Full `cargo test -p praxis-core` passes when build
   verification is allowed.

## 11. Verification Criteria

- After phase 1: `praxis-loop` passes standalone tests, zero
  `praxis-core` / `praxis-protocol` dependency.
- After phase 2: shadow tests show new and old loops produce identical results
  for identical input.
- After phase 3: existing praxis tests pass; plugin / skill / MCP / app-gateway
  show zero regression.
- Invariant: external interfaces stay 100% compatible.

## 12. Scope Boundary (what we do not do)

- This refactor is internal to `praxis-rs/`; it is the Praxis product's own
  internal restructuring.
- It does not touch `praxis-app-core`, the Cunning3D host program, or the
  `cunning3d-graph-control` plugin.
- `TurnHooks` is an internal refactor seam for testability and evolvability,
  not a product extension API. Product extension goes through plugin / skill /
  MCP, which is unchanged.
- No compatibility dual-track: once phase 3 lands, old loop code is deleted.
- Codex wire contracts (`/api/codex/*`, `CODEX_HOME`) are a separate brand
  concern handled in [`codex-compat-cleanroom.md`](../../docs/codex-compat-cleanroom.md),
  not part of this loop refactor.

## 13. Debt Resolved

| Current debt | This plan |
|---|---|
| `Arc<Session>` god object | four parameters + three-state separation |
| behavior welded into 261 lines | strongly typed hook decisions |
| hooks see the entire state | C8 Query-minimal inputs |
| missing prepareNextTurn / shouldStop | `prepare_next_round` / `after_turn_complete` |
| approval welded into tool runtime | `before_tool_call` |
| one-size-fits-all tool concurrency | `ConcurrencyMode` declarative batching |
| loop not unit-testable | standalone crate + mock services |
| `turn.rs` 2295-line monolith | loop body under 150 lines of pure orchestration |
| loop welded to protocol | self-owned abstractions + adapter conversion |

## 14. One-line Summary

Extract one model/tool turn into a standalone `praxis-loop` crate shaped as a
hand-written mini ECS: TurnState is Component, TurnServices (five traits) is
Resource, TurnHooks (strongly typed decisions) is System, turn_loop is
Schedule. Borrow ECS Query-minimal access and Plugin composition; do not adopt
the Bevy frame scheduler. Eight constraints prevent bloat; the two upper loops
and concrete runtimes stay in `praxis-core`; three-phase migration; Cunning3D
side is untouched.
