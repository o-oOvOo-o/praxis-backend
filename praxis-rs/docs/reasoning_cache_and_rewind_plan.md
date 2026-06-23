# Reasoning, Cache Stats, and Future Rewind Plan

Date: 2026-06-01

This document records the current decision after comparing Praxis, Reasonix, and Zed. The immediate work is to absorb the useful DeepSeek-facing parts of Reasonix ideas 1 and 3, clarify Praxis cache accounting, and defer idea 4 until Praxis has a stronger Zed-style thread/workspace model.

## Decision

Absorb now:

- `1`: DeepSeek thinking/reasoning handling.
- `3`: DeepSeek cache hit and token usage visibility.

Do not absorb now:

- `4`: Reasonix checkpoint/rewind.
- Generic provider matrices. OpenAI Responses keeps its native path. GLM/Qwen/common can be derived later after the DeepSeek specialization is stable.
- Google-style cache handling. It is not part of the current Praxis product path.

Reasonix checkpoint/rewind is too small for Praxis. It is closer to message truncation plus file snapshots, and it does not cleanly cover shell side effects, git state, external processes, multi-thread control, or future GUI thread management. Praxis should eventually solve rewind through its own thread history, event log, diff, and workspace state model.

## Current Praxis Cache Model

Praxis already has a usable cache vocabulary in `protocol/src/protocol.rs`:

- `TokenUsage.input_tokens`
- `TokenUsage.cached_input_tokens`
- `TokenUsage.cache_reported_input_tokens`
- `TokenUsage.output_tokens`
- `TokenUsage.reasoning_output_tokens`
- `TokenUsage.total_tokens`

The intended display semantics should be:

- `input_tokens`: provider-reported prompt/input tokens for the turn.
- `cached_input_tokens`: subset of input tokens served from provider prompt cache.
- `cache_reported_input_tokens`: input-token denominator for which the provider explicitly reported cache accounting.
- `non_cached_input_tokens`: `input_tokens - cached_input_tokens`.
- `cache_hit_percent`: `cached_input_tokens / cache_reported_input_tokens`.
- `output_tokens`: visible and non-visible model output tokens reported by provider, depending on provider semantics.
- `reasoning_output_tokens`: reasoning output token count when the provider reports it separately.
- `total_tokens`: provider total when available; otherwise local best effort.

Important: cache hit rate is a provider observation, not a promise that the next request will hit the same cache.
If a provider does not report cache accounting, `cache_reported_input_tokens` must stay `0` and the UI must omit cache hit rate instead of showing `0%`.

## Provider Usage Fields To Normalize Now

OpenAI Responses native path:

- `prompt_tokens`
- `completion_tokens`
- `prompt_tokens_details.cached_tokens`
- `completion_tokens_details.reasoning_tokens`

Praxis keeps this path working, but this document does not expand it. The hosted Responses implementation is already strong.

DeepSeek-style common API:

- `prompt_cache_hit_tokens`
- `prompt_cache_miss_tokens`
- `prompt_tokens`
- `completion_tokens`

Praxis should normalize DeepSeek usage into the same `TokenUsage` shape used by the rest of the app, then show both last-turn and session totals in TUI/GUI.

## Work Phase 1: Thinking Handling

Goal: DeepSeek thinking should be visible and persisted correctly without polluting provider replay.

Required behavior:

- OpenAI Responses reasoning summaries remain their own path.
- DeepSeek can emit full thinking when the provider returns `reasoning_content` or equivalent fields.
- GUI/TUI should distinguish summary reasoning from full reasoning:
  - `Summary`: OpenAI Responses reasoning summary.
  - `Full`: DeepSeek when real full thinking is returned.
- Full DeepSeek thinking should be shown to the user, but must not be replayed back into later DeepSeek requests.

DeepSeek is the first and only provider to harden in this phase. Praxis parses DeepSeek `reasoning_content` into full thinking blocks for display, but does not replay that full thinking content into later DeepSeek requests. Other common providers should wait until this specialization proves the right shape.

## Work Phase 2: Cache Stats Visibility

Goal: make Praxis cache behavior observable before debating cache optimization.

TUI/GUI should show:

- Last turn cache hit percentage.
- Last turn cached and non-cached input tokens.
- Session aggregate cache hit percentage.
- Session aggregate cached and non-cached input tokens.
- Output and reasoning output tokens when available.

Preferred display:

- Compact footer/status line for the current thread.
- Expanded usage panel or tooltip for detailed last/session numbers.
- Workspace view should surface cache stats per selected thread, not just globally.

Implementation notes:

- Keep cache accounting in backend protocol/state first.
- UI should only render normalized `TokenUsageInfo`; it should not parse provider-specific payloads.
- If a provider does not report cache fields, show unknown or omit cache rate, not `0%`.
- Preserve raw provider usage in debug logs only if useful; do not make UI depend on raw provider JSON.

Implemented state as of 2026-06-01:

- DeepSeek common transport normalizes `prompt_cache_hit_tokens` and `prompt_cache_miss_tokens` into `TokenUsage`.
- Thread metadata persists the latest full `TokenUsageInfo` snapshot in SQLite so historical thread lists can show cache data.
- App gateway `Thread` responses expose persisted usage through `ThreadTokenUsage`.
- Praxis Workspace thread rows prefer live token usage and fall back to persisted thread usage; selected rows show cached/reported token counts.

## Work Phase 3: Future Rewind

Rewind is deferred until after thinking and cache are stable.

Do not copy Reasonix checkpoint/rewind as the architecture. Future rewind should be closer to Zed/Praxis:

- Thread metadata separate from full thread body.
- Thread event log can reconstruct UI state.
- Tool calls, diffs, approvals, and edits are first-class events.
- Workspace/file state restoration uses git/diff/worktree-aware mechanisms, not ad hoc file snapshots.
- Multi-thread/rank control must be represented explicitly, so a controller thread can understand what it owns without seeing protected content it should not access.

Useful Zed concepts to revisit:

- Thread metadata store for sidebar/project grouping.
- Full thread store for persisted messages, usage, thinking settings, draft prompt, and scroll state.
- Thread item UI with status, worktree metadata, timestamps, and diff stats.
- Git worktree archive/restore based on WIP commits and refs, not raw folder snapshots.

## Code Landmarks

Praxis:

- `protocol/src/protocol.rs`: `TokenUsage`, `TokenUsageInfo`, cache hit helpers.
- `core/src/non_responses_transport.rs`: DeepSeek/common transport reasoning and usage parsing.
- `core/src/praxis.rs`: token usage state update and stream turn integration.
- `protocol/src/openai_models.rs`: common and DeepSeek model metadata.
- `tui/src`: TUI rendering of thinking, status, and usage.

Zed reference:

- `crates/agent/src/thread.rs`: thread thinking, usage, replay, title, summary.
- `crates/agent/src/db.rs`: persisted thread body.
- `crates/agent/src/thread_store.rs`: full thread loading/saving.
- `crates/agent_ui/src/thread_metadata_store.rs`: sidebar metadata and archived worktree links.
- `crates/ui/src/components/ai/thread_item.rs`: thread row status, worktree, diff, timestamp UI.

Reasonix reference:

- Keep only the useful observations from ideas 1 and 3.
- Do not import its checkpoint/rewind structure.

## Next Action

After manual DeepSeek testing:

1. Verify provider usage payloads from real DeepSeek runs match the normalized cache counts.
2. Tune the TUI/GUI expanded usage layout if the compact selected-row counts are not enough.
3. Then revisit whether any DeepSeek behavior should be promoted into the common profile.

Only after this should Praxis revisit rewind.
