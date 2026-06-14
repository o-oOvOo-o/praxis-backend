# Praxis Main Framework Naming Refactor

Date: 2026-06-13

This document records the agreed naming and ownership decisions for the Praxis main framework refactor. The current pass is a breaking internal naming and boundary cleanup. It must preserve behavior while making the main framework easier to read, split, and replace later.

## Main Mental Model

```text
app-gateway -> ThreadManager -> PraxisThread/Session -> main_agent_loop -> agent_task_loop -> agent_turn_loop
```

The framework has three explicit agent loop concepts:

- `main_agent_loop`: the long-lived per-thread submission loop. It consumes `Submission`, dispatches `Op`, and keeps the thread alive.
- `agent_task_loop`: the repeated task-level loop around one user turn. It handles pending input and empty-model recovery around turn execution.
- `agent_turn_loop`: the scoped model/tool loop inside one agent turn. It samples the model, executes tool calls, handles pending model/tool state, and completes the assistant response.

`AgentOs` and gateway code are not loops:

- `AgentOs`: shared cross-agent coordination, runtime commands, tickets, leases, artifacts, process tracking, and capability accounting.
- `app-gateway`: external and embedded transport/application boundary over the thread service APIs.
- `ThreadManager`: thread lifecycle owner. Spawn, fork, resume, lookup, registry, and shutdown stay here.

## Implemented Naming Table

| Old name | New name | Status | Ownership |
|---|---|---:|---|
| `app-gateway-runtime` crate | `app-gateway` crate | Done | Gateway host, not a runtime loop. |
| `praxis-app-gateway-runtime` bin | `praxis-app-gateway` bin | Done | External gateway executable. |
| `praxis_app_gateway_runtime` lib | `praxis_app_gateway` lib | Done | Gateway host library. |
| `AppGatewayRuntimeTransport` | `AppGatewayTransport` | Done | Gateway transport selector. |
| `AppGatewayRuntimeTransportParseError` | `AppGatewayTransportParseError` | Done | Gateway transport parse error. |
| `AppGatewayRuntimeArgs` | `AppGatewayArgs` | Done | Gateway CLI args. |
| `AgentOsRuntime` | `AgentOs` | Done | AgentOS is the coordination system itself. |
| `agent_os/runtime_lifecycle.rs` | `agent_os/lifecycle.rs` | Done | AgentOS lifecycle helpers. |
| `agent_os/dispatch.rs` | `agent_os/control_plane.rs` | Done | AgentOS control-plane dispatch. |
| AgentOS inline state structs | `agent_os/state.rs` | Done | Internal AgentOS state. |
| AgentOS inline managed command structs | `agent_os/managed_commands.rs` | Done | Managed command span/output/audit state. |
| `ThreadManagerState` | `ThreadManagerInner` | Done | Shared internal manager state. |
| loaded thread map inside `ThreadManager` | `ThreadRegistry` | Done | Internal registry only; no spawn ownership. |
| `NewThread` | `ThreadSpawnResult` | Done | Result returned by thread spawn. |
| UI/debug-client `NewThread` target | `StartThread` | Done | Internal UI command/target now matches `thread/start`; user command text stays `:new`. |
| `ForkSnapshot` | `ThreadForkSnapshot` | Done | Snapshot used by fork/resume. |
| `praxis/submission.rs` | `praxis/main_agent_loop.rs` | Done | Long-lived submission loop module. |
| `submission_loop` | `main_agent_loop` | Done | The real main agent loop. |
| `praxis/run_turn.rs` | `praxis/agent_turn_loop.rs` | Done | Scoped per-turn loop module. |
| `run_turn` | `agent_turn_loop` | Done | The real turn loop. |
| `praxis/runtime_lifecycle.rs` | `praxis/thread_lifecycle.rs` | Done | Praxis thread spawn/submit/shutdown helpers. |
| `SessionTask` | `AgentTask` | Done | Executable agent task abstraction. |
| `SessionTaskContext` | `AgentTaskContext` | Done | Task execution context. |
| `RegularTask` | `RegularAgentTask` | Done | Regular user-turn task. |
| `RunningTask` | `RunningAgentTask` | Done | Running task handle/state. |
| `TaskKind` | `AgentTaskKind` | Done | Agent task kind. |
| repeated turn plumbing in `RegularAgentTask` | `agent_task_loop` | Done | Task-level loop between main loop and turn loop. |
| `tools/parallel.rs` | `tools/tool_call_runtime.rs` | Done | File matches `ToolCallRuntime`. |
| tracing span `session_task.turn` | `agent_task.turn` | Done | Trace name matches task abstraction. |

## Preserved Names

| Name | Decision | Reason |
|---|---:|---|
| `Op` | Keep | Short and accurate operation submitted into a session. Matches upstream Codex and remains useful. |
| `Submission` | Keep | Accurate envelope for an `Op` plus submission metadata. |
| `Session` | Keep | Long-lived per-thread aggregate root. |
| `PraxisThread` | Keep | Public thread handle. |
| `ThreadManager` | Keep | Lifecycle service and spawn/fork/resume owner. |
| `state/session.rs` | Keep | Session configuration, history, active turn, and rate-limit state. |
| `state/turn.rs` | Keep | Per-turn state. |
| `rollout.rs` | Keep | Local conversation persistence and list/resume/fork storage. |
| `event_mapping.rs` | Keep | Backend event to gateway/thread item mapping. |
| `praxis/event_delivery.rs` | Keep | Event sending, broadcast, and subscription boundary. |
| `ToolCallRuntime` | Keep | Tool call dispatch and concurrent execution. |
| `ToolRouter` | Keep | Tool routing. |
| `AgentControl` | Keep | Sub-agent control surface. |
| `AgentRegistry` | Keep | Agent identity, role, and status registry. |
| `ModelRuntime` | Keep | Model runtime selection and provider behavior. |
| `ModelRuntimeRegistry` | Keep | Provider/profile/model capability registry. |
| `context_manager/*` | Keep | Context construction and history compaction. |
| `guardian/*` | Keep | Approval and safety review. |
| `sandboxing/*` and `exec_policy.rs` | Keep | Sandbox and execution policy. |
| `unified_exec/*` | Keep | Process execution and output collection. |
| `mcp.rs` and `praxis/mcp_runtime.rs` | Keep | MCP lifecycle. |
| `hook_runtime.rs` | Keep | Hook execution. |
| `goals.rs` | Keep | Goal state. |
| `plugins/*` | Keep | Plugin management. |

## Ownership Rules

- `ThreadManager` owns thread lifecycle. Spawn, fork, resume, shutdown, and registry mutation stay here.
- `ThreadRegistry` is an internal lookup component inside `ThreadManager`; it must not become a second lifecycle owner.
- `main_agent_loop` consumes `Submission` and dispatches `Op`; it does not create threads.
- `agent_task_loop` handles task-level repetition around a turn; it does not own the thread or tool runtime.
- `agent_turn_loop` owns one turn of model/tool execution; it does not own thread lifecycle.
- `AgentOs` owns cross-agent coordination and managed side effects; it is not the thread lifecycle manager.
- `ToolCallRuntime` owns tool-call dispatch and concurrency; it does not become a generic capability abstraction.
- Gateway crates stay thin over core service APIs and must not duplicate lifecycle logic.

## Refactor Benefits

- The three real loops are visible by name and file path.
- Thread lifecycle ownership stays centralized, so future GUI, TUI, Harness, and external agent control paths do not fork lifecycle logic.
- AgentOS remains a coordination system instead of a catch-all runtime name.
- Gateway naming now describes the actual boundary instead of implying a runtime loop.
- `ThreadRegistry` makes loaded-thread lookup explicit without moving spawn ownership away from `ThreadManager`.
- Tool-call concurrency has a file name that matches the `ToolCallRuntime` abstraction.
- Later destructive main-framework replacement can happen module by module without first decoding old misleading names.

## Main Framework Refactor Pass 1

Started: 2026-06-13

Completed static-only changes:

- Slimmed `main_agent_loop` into an explicit receive/span/dispatch loop.
- Extracted `dispatch_submission` and `dispatch_op` so the long-lived loop is no longer the full operation router.
- Extracted submission error emission into `send_submission_error`.
- Converted `OverrideTurnContext` handling into an internal `OverrideTurnContextUpdate` path.
- Split `AgentTask` finish handling into state collection, pending input recording, metrics emission, and pending-work continuation scheduling.
- Extracted dynamic tool restoration for start/resume/fork into `resolve_dynamic_tools_for_session`.

Behavior preserved:

- Gateway APIs still call the same core lifecycle entry points.
- `ThreadManager` still owns spawn/fork/resume/shutdown.
- `main_agent_loop` still consumes `Submission` and returns `true` only on shutdown.
- Dynamic tool priority remains explicit start tools, state DB, rollout history, then empty.
- Turn completion still emits the same metrics, events, runtime-command completion, cost estimate, auto title, auto summary, and idle continuation.

## Main Framework Refactor Pass 2

Started: 2026-06-13

Completed static-only changes:

- Moved pending input inspection and recording behind `hook_runtime::record_pending_inputs`.
- Moved sampling-time pending input handling behind `hook_runtime::process_pending_input_for_sampling`.
- Made pending input hook records private to `hook_runtime`.
- Moved missing-final-answer synthesis behind `stream_events_utils::synthetic_final_item_for_guard`.
- Moved synthetic final answer emission behind `stream_events_utils::emit_synthetic_final_answer`.
- Made low-level synthetic final item builders private to `stream_events_utils`.
- Collapsed repeated token usage histogram emission into one task metric helper.

Behavior preserved:

- Task finish still records every queued pending input item before turn completion.
- Sampling still records accepted pending input before model sampling.
- Sampling still requeues remaining input after the first hook-blocked pending item.
- Sampling still retries without model sampling when blocked before any accepted input and more input remains.
- Sampling still stops without model sampling when blocked before any accepted input and no input remains.
- Model-empty synthetic final output is still task-finish only.
- The turn loop still only synthesizes final output for terminal `list_agents` and sub-agent workflow guards.
- Synthetic final answers still emit started/completed turn items, record the completed response item, and return the last assistant message through the same parsing path.
- Turn token usage metrics still emit the same metric name, token-type labels, values, and temporary memory label.

## Main Framework Refactor Pass 3

Started: 2026-06-13

Completed static-only changes:

- Added `CompletedResponseItemSink` as the shared completed-response item lifecycle boundary.
- Routed hidden tool-loop final output, ordinary non-tool output completion, synthetic final answers, and plan-mode assistant record/last-message extraction through the sink.
- Added `history_preview::HistoryPreview` as the first shared history snapshot/query boundary.
- Moved latest-user-message preview and requested-final-line-marker lookup behind `HistoryPreview`.

Behavior preserved:

- Completed response items still write to history, rollout, raw response item events, memory pollution tracking, and stage-1 citation usage.
- Completed turn items still emit started before completed when no active streamed item exists.
- Image generation completed items still synthesize an in-progress started item before the completed item.
- Plan-mode streaming still owns its deferred agent-message start/completion state; only record/last-message extraction was shared.
- Requested final-line marker detection keeps the same prefixes, case handling, `rfind` behavior, and backtick trimming.
- Empty-model recovery still uses the latest non-contextual user message truncated to the same 2000-token policy.

## Main Framework Refactor Pass 4

Started: 2026-06-13

Completed static-only changes:

- Expanded `history_preview::HistoryPreview` into the shared history snapshot/query boundary.
- Moved first-user-message lookup, auto-title preview construction, auto-summary preview construction, and realtime current-thread-section construction behind `HistoryPreview`.
- Kept `auto_title::title_preview_from_response_items` as a compatibility wrapper over `HistoryPreview` for existing callers.
- Added `ContextManager::into_raw_items` so `HistoryPreview::for_session` avoids cloning the history vector twice.

Behavior preserved:

- Auto-title still skips bootstrap context messages and keeps the same preview message and character limits.
- Provisional title generation still uses the same bootstrap-context filter and heuristic title logic.
- Auto-summary still deduplicates the last assistant message, keeps the same recent-message count, prompt truncation limits, and heuristic summary input shape.
- Realtime startup context still keeps the same current-thread turn grouping, contextual-user filtering, section header text, and max turn count.
- History preview construction now owns one cloned history snapshot and performs all preview queries from that snapshot.

## Main Framework Refactor Pass 5

Started: 2026-06-13

Completed static-only changes:

- Added `app-gateway::realtime_event_bridge` for realtime core event to app-gateway notification mapping.
- Added `ThreadItemNotificationSink` as the shared app-gateway thread item lifecycle sender.
- Routed dynamic tools, MCP tool calls, collab tools, image view, web search, patch/apply file changes, exec completion, hook prompts, generic item started/completed, and review-mode item notifications through the shared sink where they emit `ItemStarted` or `ItemCompleted`.
- Added `app-gateway::collab_agent_event_bridge` as the single collab event to `ThreadItem::CollabAgentToolCall` mapping boundary.
- Slimmed `bespoke_event_handling.rs` so collab branches keep thread-control side effects locally while item construction lives in the bridge.

Behavior preserved:

- Realtime notification payloads keep the same type strings and fields.
- Thread item lifecycle notifications keep the same thread id, turn id, and `ThreadItem` payloads.
- Dynamic tool notifications still use the turn id carried by the dynamic tool event, not the outer event turn id.
- Collab thread-control acquire/release and close notifications stay in `bespoke_event_handling.rs`.
- Collab item mapping keeps the previous status, receiver id, prompt, model, reasoning effort, and agent-state behavior.
- Raw response item completion remains a distinct notification path and is not forced through the item sink.

## Main Framework Refactor Pass 6

Started: 2026-06-13

Completed static-only changes:

- Added `ToolLifecycleEmitter` as the shared non-exec tool lifecycle event boundary in `tools/events.rs`.
- Routed `web_search` begin/end events through `ToolLifecycleEmitter`.
- Routed MCP resource begin/end events through `ToolLifecycleEmitter`.
- Routed ordinary MCP tool call begin/end, approval skip, safety skip, and app-policy skip events through `ToolLifecycleEmitter`.
- Added `finish_mcp_resource_call` to collapse the repeated MCP resource payload/serialization/error completion branches.
- Removed handler-local MCP resource event helpers and the generic `notify_mcp_tool_call_event` wrapper.

Behavior preserved:

- Web search still emits the same begin call id and end query/action payload.
- MCP resource tools still emit the same invocation, duration, success mapping, and error string payloads.
- Ordinary MCP tool calls still emit begin before approval, emit zero-duration skip endings for blocked/declined paths, and keep metrics and app-usage tracking unchanged.
- MCP tool result sanitization, approval decisions, and model-facing results are unchanged.

## Main Framework Refactor Pass 7

Started: 2026-06-13

Completed static-only changes:

- Added `multi_agents::events::CollabAgentEventEmitter` as the shared collaboration event boundary for multi-agent tools.
- Routed `spawn_agent` begin/end events through the shared emitter.
- Routed targeted and global `wait_agent` begin/end events through the shared emitter.
- Routed `send_message` and `assign_task` interaction begin/end events through the shared emitter.
- Routed `close_agent` begin/end events, including the status-subscription error branch, through the shared emitter.
- Removed direct collaboration protocol event construction from multi-agent handler files outside `events.rs`.

Behavior preserved:

- Spawn events keep the same prompt, requested/effective model, reasoning effort, new-agent identity fields, and status payloads.
- Wait events keep the same sender, receiver thread ids, receiver metadata, and status map payloads.
- Message interaction events keep the same receiver id, interaction kind, prompt, receiver metadata, and status payloads.
- Close events keep the same begin/end ordering and still emit an end event before returning subscription errors.
- Tool business logic, AgentOS dispatch, inter-agent communication, target resolution, and model-facing outputs are unchanged by this pass.

## Main Framework Refactor Pass 8

Started: 2026-06-13

Completed static-only changes:

- Added `make_warning_event`, `make_error_event`, and `make_deprecation_notice_event` in `praxis::event_delivery`.
- Added `SessionEventEmitter` for raw event-id scoped warning/error delivery.
- Added `TurnEventEmitter` for turn-context scoped warning/error delivery.
- Routed startup deprecation notices, startup warnings, hook startup warnings, submission errors, session update errors, memory command events, rollback errors, thread-name errors, shutdown recorder errors, review resolution errors, resume-model warnings, model-metadata warnings, fallback-transport warnings, skill warnings, hook abort warnings/errors, invalid-image errors, and terminal model errors through the shared emitters.
- Removed local `WarningEvent`/`ErrorEvent` imports from `praxis::handlers` after migrating its direct event construction.

Behavior preserved:

- Raw events still go through `send_event_raw`, preserving rollout persistence and direct client delivery.
- Turn events still go through `send_event`, preserving legacy event mirroring, realtime handoff handling, parent completion notifications, and rollout persistence.
- Existing event ids, turn ids, warning text, error text, and `CodexErrorInfo` payloads are unchanged for migrated call sites.
- Stream error events, pattern matches over event variants, hook started/completed events, model reroute events, thread rollback notifications, and thread-name update notifications remain explicit because they are not simple warning/error/deprecation payload construction.

## Main Framework Refactor Pass 9

Started: 2026-06-13

Completed static-only changes:

- Added `app-gateway::client_response_decode` as the shared typed client-response decoding boundary.
- Centralized client request receive errors, client JSON-RPC errors, turn-transition cancellation detection, and typed JSON deserialization fallback logging.
- Routed `ToolRequestUserInputResponse`, `McpServerElicitationRequestResponse`, `PermissionsRequestApprovalResponse`, `FileChangeRequestApprovalResponse`, and `CommandExecutionRequestApprovalResponse` decoding through the shared boundary.
- Removed the repeated local `serde_json::from_value + log + default response` blocks for the migrated gateway response handlers.
- Removed unused decoder helpers during the pass so the new abstraction contains only currently used entry points.

Behavior preserved:

- Request-user-input still submits an empty answer map on client/receive/decode fallback and still returns without submitting on turn transition.
- MCP elicitation still maps turn transition to `Cancel`, other client/receive/decode fallback to `Decline`, and preserves content/meta on valid responses.
- Request-permissions still returns without submitting on turn transition and still falls back to an empty turn-scoped grant on client/receive/decode fallback.
- File-change approvals still treat client/receive fallback as failed completion and decode fallback as declined approval.
- Command-execution approvals still treat client/receive fallback as failed completion and decode fallback as declined approval.

## Main Framework Refactor Pass 10

Started: 2026-06-13

Completed static-only changes:

- Added `app-gateway::approval_response_bridge` for API approval response to core `ReviewDecision` plus completion-status mapping.
- Moved file-change approval response outcome mapping out of `bespoke_event_handling.rs`.
- Moved command-execution approval response outcome mapping out of `bespoke_event_handling.rs`.
- Kept thread-item completion, subcommand suppression, pending-request resolution, guard release, and `PraxisThread::submit` side effects in `bespoke_event_handling.rs`.
- Moved test-only `FileChangeApprovalDecision` and `ReviewDecision` imports into the test module after extracting the mapper.

Behavior preserved:

- File-change accept/accept-for-session/decline/cancel mapping is unchanged.
- File-change client/receive fallback still returns denied plus failed completion; decode fallback still returns denied plus declined completion.
- Command accept/accept-for-session/execpolicy amendment/network policy amendment/decline/cancel mapping is unchanged.
- Command client/receive fallback still returns denied plus failed completion; decode fallback still returns denied plus declined completion.
- Turn-transition cancellation still returns from the caller without submitting approval responses.

## Main Framework Refactor Pass 11

Started: 2026-06-13

Completed static-only changes:

- Added `app-gateway::mcp_tool_event_bridge` as the MCP tool call item mapping boundary.
- Moved MCP tool begin/end `ThreadItem::McpToolCall` construction out of `bespoke_event_handling.rs`.
- Kept MCP tool item start/completion delivery in `bespoke_event_handling.rs`.
- Moved MCP tool item mapping tests next to the bridge module.
- Removed MCP tool status/result/error/event internals from the `bespoke_event_handling.rs` test module.

Behavior preserved:

- MCP tool begin items keep the same id, server, tool, in-progress status, arguments/null fallback, and empty result/error/duration fields.
- MCP tool end items keep the same success/failed status mapping, result/error mapping, arguments/null fallback, and millisecond duration conversion.
- Gateway event handling still starts and completes MCP tool thread items through the existing item sink.

## Main Framework Refactor Pass 12

Started: 2026-06-13

Completed static-only changes:

- Renamed `tui::app_gateway_approval_conversions` to `tui::app_gateway_core_conversions`.
- Centralized app-gateway request id to MCP request id conversion for `app.rs`, pending request tracking, and interactive replay.
- Centralized app-gateway command approval decision to core `ReviewDecision` conversion.
- Moved app-gateway command approval, file-change approval, permissions request, user-input request, patch-change, collab-state, collab-thread-id, and web-search action conversion helpers out of `chatwidget.rs`.
- Routed app-gateway snapshot web-search conversion through the shared TUI conversion boundary.
- Kept metadata-enriched collab rendering assembly in `chatwidget.rs`, because it depends on widget-owned cached agent metadata.

Behavior preserved:

- Command approval prompts keep the same command parsing, cwd fallback, reason, turn id, approval id, network context, additional permissions, policy amendments, available decisions, and parsed command actions.
- File-change approvals keep the same empty change map for request prompts and the same concrete patch-change mapping for rendered items.
- Permissions and request-user-input prompts keep the same call ids, turn ids, reason text, permission profile conversion, questions, options, and secret/other flags.
- Collab state mapping keeps the same pending/running/interrupted/completed/errored/shutdown/not-found behavior and invalid-thread warning.
- Web-search action mapping keeps the same search/open/find/other variants.

## Main Framework Refactor Pass 13

Started: 2026-06-13

Completed static-only changes:

- Added `app-gateway::server_request_lifecycle` as the shared pending server-request lifecycle boundary.
- Moved thread-listener ordered pending-request resolution out of `bespoke_event_handling.rs`.
- Added `PendingServerRequest` to carry app-gateway request id plus client response receiver together.
- Added `send_server_request` so app-gateway request creation no longer repeats tuple unpacking at each handler.
- Routed file-change approval, command-execution approval, request-user-input, MCP elicitation, and request-permissions response handlers through `PendingServerRequest::await_response_and_resolve`.
- Left dynamic tool calls on the direct request path because they do not use thread pending-state guards or thread-listener ordered resolve notifications.

Behavior preserved:

- Client responses are still awaited before resolving the server request on the thread listener.
- Pending request resolved notifications still go through `ThreadListenerCommand::ResolveServerRequest`.
- Permission and user-input guards are still dropped immediately after pending request resolution and before response decode/submit work.
- Each handler still owns its existing fallback decode, completion item update, and `PraxisThread::submit` behavior.

## Main Framework Refactor Pass 14

Started: 2026-06-13

Completed static-only changes:

- Moved thread turn hydration ownership from `thread_listener_api` into `thread_projection_api`.
- Added `ThreadTurnSource` as the shared projection source for rollout paths and already-loaded rollout items.
- Added `read_turns_from_rollout` so direct rollout-to-turn conversion is centralized.
- Routed `thread/read` include-turns loading, listener pending-turn replay, resume response hydration, fork response hydration, and rollback response hydration through the shared projection helpers.
- Removed direct `read_rollout_items_from_rollout -> build_turns_from_rollout_items -> thread.turns` call sites outside `thread_projection_api`.

Behavior preserved:

- `thread/read includeTurns` still returns the same not-materialized invalid-request error for missing rollout files.
- Listener replay still merges the active turn after loading persisted turns.
- Resume and fork responses still use already-loaded initial history when available.
- Ephemeral fork responses still compute preview from the source rollout items before hydrating visible turns.
- Rollback response thread status, resolved thread name, and rollout summary loading behavior are unchanged.

## Main Framework Refactor Pass 15

Started: 2026-06-13

Completed static-only changes:

- Added a shared `MessageProcessor::send_result_response` helper for plain app-gateway JSON-RPC result responses.
- Added a shared `PraxisMessageProcessor::send_result_response` helper for Praxis API submodules.
- Replaced repeated `Ok(response) => send_response / Err(error) => send_error` blocks in plain config read, requirements read, external-agent config, filesystem, filesystem watch, and command exec write/resize/terminate handlers.
- Kept config mutation handlers explicit because they clear plugin caches, start plugin tasks, and sometimes refresh the app list after successful writes.

Behavior preserved:

- Successful responses still call `OutgoingMessageSender::send_response` with the original request id.
- Error responses still call `OutgoingMessageSender::send_error` with the original JSON-RPC error.
- Command exec write/resize/terminate still pass a cloned request id into the command manager before responding on the original request id.
- Config mutation side effects still happen before successful write responses and are not shared with read-only handlers.

## Main Framework Refactor Pass 16

Started: 2026-06-13

Completed static-only changes:

- Added `PraxisMessageProcessor::parse_thread_id` as the shared JSON-RPC thread id parsing boundary.
- Routed `load_thread` through the shared parser.
- Replaced repeated invalid-thread-id response construction in feedback upload, thread archive/delete/unarchive, thread control acquire/release, thread goal lookup, thread name updates, thread metadata updates, thread read, rollout resume, fork source resolution, and thread unsubscribe.
- Routed plain config value writes through the existing config mutation response helper instead of open-coding the same cache-refresh/send-response branch.

Behavior preserved:

- Invalid thread id responses keep the same `INVALID_REQUEST_ERROR_CODE` and message text.
- APIs that need their own not-found, not-loaded, or rollout-path error messages still own those messages.
- Optional feedback thread ids still remain optional and only fail when a supplied id is malformed.
- Config value writes still clear plugin-related caches, start plugin startup tasks, and send the successful write response in the same order as batch writes.
- Cursor parsing, optimistic running-thread checks, and special resume/fork probes still keep their local `ThreadId::from_string` use because they are not simple request-error boundaries.

## Main Framework Refactor Pass 17

Started: 2026-06-13

Completed static-only changes:

- Added `app-gateway::json_rpc_error` as the shared JSON-RPC error construction boundary.
- Routed `PraxisMessageProcessor::load_thread`, `parse_thread_id`, `load_latest_config`, `send_invalid_request_error`, and `send_internal_error` through the shared error constructors.
- Added `thread_rollout_locator` under `praxis_message_processor` to own thread id plus rollout archived-scope lookup.
- Replaced repeated rollout lookup branches in thread archive, delete, unarchive, running resume, rollout resume, and fork source resolution.
- Routed archive/delete/unarchive path validation and filesystem errors through the shared JSON-RPC error constructors.
- Preserved `ThreadDirectory::read_history_cwd` as a local fork concern because it is not just path lookup.

Behavior preserved:

- Invalid request and internal error codes remain unchanged.
- Invalid thread id, thread-not-found, reload-config, missing-rollout, locate-failed, and archived-rollout error text stays equivalent.
- Running thread resume still prefers the live rollout path when it exists and falls back to directory lookup otherwise.
- Fork still reads history cwd from `ThreadDirectory` after resolving the source rollout path.
- The new rollout locator remains inside app-gateway because it returns JSON-RPC errors and should not leak transport semantics into rollout storage.

## Main Framework Refactor Pass 18

Started: 2026-06-13

Completed static-only changes:

- Added TUI Center row maintenance helpers on `App`: `resort_center_thread_rows`, `clamp_center_thread_rows`, `update_center_thread_row`, and `remove_center_thread_row`.
- Routed Center thread upsert, observed-thread state refresh, server notification handling, pin toggles, control release, local rename, archive, and delete through those helpers.
- Added `parse_center_thread_id` as the Center-local thread id parsing boundary.
- Replaced direct Center notification thread id parsing in `apply_center_server_notification` and token-usage replay caching.

Behavior preserved:

- Center row ordering still uses active thread, pinned ids, row priority, and updated-at timestamp.
- Center selection and scroll clamping still run after row resort/removal.
- Invalid or missing Center notification thread ids still request a Center refresh by returning `true`.
- Token usage notifications still update the per-thread usage cache even when no visible row exists.
- Batch `thread/list` refresh still owns full-list replacement and keeps its direct sort call.

## Main Framework Refactor Pass 19

Started: 2026-06-13

Completed static-only changes:

- Added `parse_app_gateway_thread_id` as the TUI app-gateway adapter's lossy thread id parser.
- Added `server_notification_thread_id` as the single `ServerNotification -> thread id string` ownership table.
- Routed `server_notification_thread_target` through the shared thread-id extractor.
- Added `App::center_observable_notification_thread_id` so Center observation decisions no longer open-code notification thread parsing.
- Routed `server_request_thread_id` through the shared parser after extracting the request thread id string.
- Routed test-only `server_notification_thread_events` through the same notification thread-id extractor before building replay events.

Behavior preserved:

- Thread-scoped notifications still route to the primary thread when no primary thread is known, otherwise to the matching thread channel.
- Notifications with malformed thread ids still warn and are ignored.
- Global notifications still go directly to `ChatWidget::handle_server_notification`.
- Center auto-observation still observes new thread starts and only observes status/control changes when the Center row says it should.
- Server requests without a thread id are still ignored with the existing warning.
- Snapshot thread replay still parses the `Thread` object's id separately because that is not a notification/request routing decision.

## Main Framework Refactor Pass 20

Started: 2026-06-13

Completed static-only changes:

- Added `find_thread_rollout_path_or_not_found` to the app-gateway thread rollout locator.
- Routed `thread_metadata_api::ensure_thread_metadata_row_exists` through shared JSON-RPC error constructors.
- Replaced the metadata path's direct `ThreadDirectory::find_rollout_path` branch with the new locator helper.
- Removed the local `invalid_request` and `internal_error` functions from `thread_metadata_api`.

Behavior preserved:

- Missing metadata rollout still reports `thread not found: {thread_id}` as an invalid request.
- Directory lookup failures in metadata reconciliation still report internal errors.
- Loaded ephemeral threads still reject metadata updates with the same invalid-request text.
- Loaded non-ephemeral threads still reconcile from their live rollout path before synthesizing metadata.
- Thread name read/write paths still use `ThreadDirectory` directly because they are not rollout path lookup.

## Main Framework Refactor Pass 21

Started: 2026-06-13

Completed static-only changes:

- Routed `command_exec_api` validation and execution setup failures through shared JSON-RPC error constructors.
- Routed `mcp_server_api` serialization, OAuth login, server lookup, transport validation, and cursor errors through shared JSON-RPC error constructors.
- Removed direct app-gateway error-code imports from both API modules.

Behavior preserved:

- `command/exec` empty command and sandbox-policy errors remain invalid requests.
- `command/exec` parameter-combination and negative-timeout errors remain invalid params.
- Managed network proxy startup and exec request build failures remain internal errors.
- MCP server-not-found, unsupported OAuth transport, invalid cursor, and cursor overflow errors remain invalid requests.
- MCP serialization and OAuth login failures remain internal errors.

## Main Framework Refactor Pass 22

Started: 2026-06-13

Completed static-only changes:

- Added the `tui/src/center` module as the ownership boundary for Praxis Center data and visible-thread state.
- Moved Center thread row construction, source/subagent extraction, row status helpers, thread id parsing, and row sorting into `center/thread_row.rs`.
- Moved Center state, overlays, context menu state, selection clamping, load-more state, and visible-tree construction into `center/state.rs`.
- Registered the new `center` module from `tui/src/lib.rs`.
- Removed the migrated Center type and helper definitions from `tui/src/app.rs`.

Behavior preserved:

- Center row construction still uses the same app-gateway thread fields, fallback name rules, preview fallback, token usage conversion, and subagent parent/depth extraction.
- Center row sorting still orders by active thread, pinned state, controlled/running priority, and updated-at timestamp.
- Center visible-tree construction still hides closed subagents behind the closed-subagents row until expanded.
- Center selection, scroll clamping, load-more row detection, and overlay state transitions still expose the same methods to `App`.
- No gateway, core, cargo, or runtime behavior was changed in this pass.

## Main Framework Refactor Pass 23

Started: 2026-06-13

Completed static-only changes:

- Added `tui/src/center/row_presentation.rs` as the Center-owned presentation boundary for thread row labels, markers, tree prefixes, indent, and control detail text.
- Moved Center closed-subagent row labels and detail text out of `tui/src/app.rs`.
- Moved Center row control marker/status label formatting out of `tui/src/app.rs`.
- Moved Center subagent tree prefix/depth/indent calculation out of `tui/src/app.rs`.
- Removed the presentation-only `ThreadActiveFlag`, `ThreadControlState`, and `ThreadControllerKind` imports from `tui/src/app.rs`.

Behavior preserved:

- Center row status strings remain `WAIT`, `RUN`, `LOCK`, `IDLE`, `ERR`, and `COLD` with the same control suffix behavior.
- Closed-subagent row copy remains language-dependent through `UiLanguage`.
- Subagent indentation still uses the same step and maximum indent values.
- Control detail still reports controller kind, rank, label, mode, read-only marker, and reason with the same English/Chinese language branches.
- Actual Center layout/rendering remains in `tui/src/app.rs`; only the row presentation rules moved.

## Main Framework Refactor Pass 24

Started: 2026-06-14

Completed static-only changes:

- Renamed the TUI ownership boundary from Center to Workspace because this code is the Praxis TUI workspace surface, not a product-wide center concept.
- Moved the TUI module boundary from `tui/src/center` to `tui/src/workspace`.
- Renamed `App::center` to `App::workspace` and routed the workspace state through the same existing app orchestration paths.
- Renamed the embedded app-gateway bootstrap helper from `ensure_local_app_gateway_for_center` to `ensure_local_app_gateway_for_workspace`.
- Kept picker-specific names (`SessionPickerAction`, `SessionPickerState`) because they describe focused interaction state, not the outer workspace shell.

Behavior preserved:

- Thread rows, selected thread state, open/resume/fork picker routing, worker board routing, launch strip state, transcript cache, and work panel rendering keep the same behavior.
- `Alignment::Center` remains ratatui layout vocabulary and is not part of the Praxis ownership naming.
- Historical Pass 22 and Pass 23 notes intentionally keep the old Center wording because they record the name used during those earlier refactor passes.

## Next Main-Framework Refactor Track

1. Keep behavior stable and continue splitting `AgentOs` by ownership: state, commands, runtime commands, leases, tickets, tasks, processes, artifacts, read model, and control plane.
2. Slim `main_agent_loop` until it reads as submission dispatch plus lifecycle bookkeeping.
3. Keep `agent_task_loop` focused on task repetition, pending input, and empty-model recovery.
4. Keep `agent_turn_loop` focused on model/tool execution and move unrelated policy/service wiring outward.
5. Audit gateway handlers so they are thin transport adapters over `ThreadManager` and core services.
6. Audit persistence/list/resume/fork code separately; `rollout.rs` can be replaced later if the storage abstraction changes.
7. Do static old-name sweeps after each pass before any runtime validation.
