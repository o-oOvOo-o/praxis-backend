## TUI Workspace State Machine Cleanup

Date: 2026-06-15

This note records the Round 1 through Round 5 cleanup for the Praxis TUI
workspace pane, picker, and overlay state model.

### Round 1

- Moved session picker and agent picker key/mouse effects behind
  `WorkspaceMainPaneEffect`, with workspace-owned resolution from
  picker-local effects.
- Added `WorkspacePane::is_picker()` and
  `WorkspaceState::is_picker_open()` as semantic picker-mode checks.
- Documented the picker effect hierarchy in `workspace/effects.rs`.

### Round 2

- Removed `WorkspaceFocus` entirely. It had no external consumer and
  duplicated state already represented by `WorkspaceOverlay`,
  `search_focused`, and `WorkspacePane`.
- Removed `previous_focus`. Once `WorkspaceFocus` was gone, picker
  open/close no longer needed a save/restore field.
- Removed the dead `WorkerBoard` pane and `WorkspaceWorkerPane`.
  The variant was matched in `app.rs` but was never constructed,
  opened, or rendered.
- Made `WorkspacePane` and `WorkspaceState::main_pane` private to
  the workspace module. App code now asks workspace state to perform
  picker operations instead of matching pane variants directly.
- Added `WorkspaceState::render_picker_pane()`. The app render path
  now asks workspace to render a picker if active; otherwise it
  renders embedded chat.
- Removed unused workspace reexports for picker render functions,
  picker state types, and `WorkspacePane`.

### Round 3

- Flattened app-facing picker effects. `app.rs` now handles one
  `WorkspaceMainPaneEffect` enum instead of nested session-picker and
  agent-picker effect enums.
- Kept picker-local effects private to workspace state resolution:
  `SessionPickerEffect` and `AgentPickerEffect` are converted inside
  `WorkspaceState`.
- Removed the app-level `handle_workspace_session_picker_effect` and
  `handle_workspace_agent_picker_effect` helpers. The app now has one
  handler for workspace main-pane effects.
- Added `WorkspaceMainPaneEffect::error_context()` so app error
  reporting no longer matches picker internals just to name an error
  source.

### Round 4

- Split app-facing main-pane effects into a no-op wrapper plus
  `WorkspaceGatewayEffect`. Workspace now explicitly names operations
  that must cross into app/gateway side effects.
- Added `WorkspaceMainPaneEffect::into_gateway_effect()` so app code
  has one gateway-effect boundary instead of matching pane effect
  variants directly.
- Moved frame scheduling policy for page-loading effects into
  `WorkspaceGatewayEffect::schedules_frame_after_apply()`.
- Kept the actual async work in `app.rs`, because it owns TUI frame
  requests, session selection, app-gateway handles, and active chat
  replacement.

### Round 5

- Added `SessionPickerPageLoaders` as the workspace-owned registry
  for session-picker page loader channels.
- Removed the raw `workspace_session_picker_loaders` map from
  `App`. App now asks `WorkspaceState` to clear, register, and queue
  picker page loader requests.
- Moved picker worker send-failure cleanup and user-facing queue
  failure messages into workspace code.
- Kept picker gateway creation in `app.rs`, because that path still
  needs app config, remote gateway target selection, app-event
  sending, and async task spawning.

### Current State Model

Workspace mode is now represented by three independent fields:

- `main_pane`: private workspace pane state (`Chat`, `SessionPicker`,
  or `AgentPicker`).
- `overlay`: workspace chrome/context/prompt overlays.
- `search_focused`: toolbar search focus.
- `session_picker_page_loaders`: private page-loader channel
  registry for active session picker sources.

This keeps picker routing, picker render, picker-local effect
resolution, and gateway-operation classification inside the workspace
boundary while leaving `app.rs` responsible for executing async side
effects, outer TUI orchestration, and embedded chat rendering.

### Preserved Behavior

- `/resume`, `/codex`, and `/cursor` still open the session picker
  through `open_session_picker`.
- Agent picker open/select semantics are unchanged.
- Picker key handling, mouse activation, scroll behavior, and page
  loading still dispatch through `WorkspaceMainPaneEffect`.
- Chat rendering still uses the same embedded chat renderer when no
  picker is active.
