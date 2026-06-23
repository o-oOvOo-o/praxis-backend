# Praxis

Praxis is a Rust-native agent platform: a general agent backend kernel, a
multi-thread control system, a plugin capability platform, a multi-provider LLM
runtime, and a shared control plane for TUI, GUI, CLI, and external agents.

This workspace is not organized around one vendor or one UI. The core language
is Agent, Thread, Session, Task, Turn, Tool, Provider, Wire, Plugin, Product,
Gateway, and Store. Product-specific behavior belongs in product profiles or
plugins. External agent formats belong behind migration adapters.

## Current Position

Praxis is being refactored toward a clean, Praxis-first architecture:

```text
Interface Surfaces
  -> App Gateway
  -> Thread Control
  -> Agent Session
  -> main_agent_loop
  -> Agent Task
  -> agent_task_loop
  -> agent_turn_loop
  -> LLM Runtime / Tools Runtime / Plugin System / ThreadStore
```

The three real agent loops are:

| Loop | Owns |
|---|---|
| `main_agent_loop` | Long-lived session submission and operation dispatch. |
| `agent_task_loop` | Repeated turn control for one task. |
| `agent_turn_loop` | One model/tool turn: sample, stream, execute tools, record results. |

`AgentOs` is not a loop. It is the cross-thread coordination plane for rank,
lease, mailbox, managed commands, artifacts, and multi-agent control.

## Key Crates

| Crate or directory | Role |
|---|---|
| `core/` | Agent kernel, sessions, thread manager, AgentOS, LLM runtime, tools, plugins, external migration. |
| `app-gateway-protocol/` | Stable control protocol types. |
| `app-gateway/` | Transport-neutral request dispatch and protocol projection. |
| `app-gateway-native/` | In-process control path for Praxis-owned UI surfaces. |
| `app-gateway-service/` | External control service such as websocket/stdout-facing hosts. |
| `app-gateway-client/` | Shared client facade used by UI and automation surfaces. |
| `tui/` | Praxis terminal UI and Center workspace. |
| `cli/` | Command entrypoint that wires TUI, exec, gateway, MCP, sandbox, and utility commands. |
| `exec/` | Non-interactive execution surface. |
| `protocol/` | Core agent protocol and event types. |
| `plugin/`, `plugins/`, `core-skills/`, `skills/` | Plugin manifests, marketplace support, built-in skills, and skill loading. |
| `rollout/`, `state/` | Persistence foundations that are being pulled behind ThreadStore-style ownership. |
| `system_plugin/3rd/openai_sandbox/`, `exec/`, `exec-server/`, `process-hardening/` | Command execution and sandbox infrastructure. |
| `otel/`, `analytics/`, `feedback/` | Observability, telemetry, diagnostics, and feedback boundaries. |

## Running Praxis

Run the TUI directly:

```shell
praxis
```

Run with an explicit remote App Gateway:

```shell
praxis app-gateway --listen ws://127.0.0.1:4222
praxis --remote ws://127.0.0.1:4222
```

Run non-interactively:

```shell
praxis exec "Summarize this repository"
echo "input text" | praxis exec "Summarize stdin"
```

Use `praxis exec --ephemeral ...` when the run should not persist session
rollout files.

## App Gateway

App Gateway is the control plane for threads, turns, events, command execution,
config, plugins, and external observers. Praxis-owned UI surfaces should prefer
the native in-process path. External agents can connect through the service
gateway.

The goal is one control model with multiple transports:

```text
TUI / GUI / CLI -> native client
External agents -> service gateway
Both -> App Gateway protocol -> Praxis core
```

## Threads, Resume, and Migration

Praxis treats a thread as the durable unit of agent work. Thread lifecycle
ownership belongs to `ThreadManager`; loaded-thread lookup belongs to
`ThreadRegistry`; persistence, list/read/resume/fork/import, lazy replay, and
metadata are moving behind ThreadStore-style APIs.

External agent formats are anti-corruption inputs:

```text
Codex rollout / Cursor store / Claude session data
  -> external_agent_migration
  -> Praxis thread history and ThreadStore records
```

The TUI should render picker and transcript view models; it should not parse raw
external stores or own resume/fork semantics.

## LLM Runtime

Praxis separates concepts that are often mixed together:

| Concept | Meaning |
|---|---|
| Provider | Account, auth, endpoint, headers, model list. |
| Wire | Network request/response shape such as Responses, Claude messages, or OpenAI-compatible JSON. |
| Behavior profile | Model-family behavior: tool format, thinking, summaries, prompt constraints. |
| Product profile | Product-level overlays such as Praxis or Cunning3D prompt/tool policy. |
| Plugin | External capability, skill, hook, app, model catalog, marketplace, or product extension. |

Rule:

```text
Wire != Provider
Provider != Behavior
Behavior != Product
Product != Plugin
```

## Tools and Sandbox

Tool execution belongs behind the tools runtime: registry, router, tool call
runtime, approval, sandbox policy, network approval, output reduction, and
concrete tool handlers.

Sandbox commands are available for platform debugging:

```shell
praxis sandbox macos [--full-auto] [--log-denials] [COMMAND]...
praxis sandbox linux [--full-auto] [COMMAND]...
praxis sandbox windows [--full-auto] [COMMAND]...
```

The main CLI also accepts `--sandbox`:

```shell
praxis --sandbox read-only
praxis --sandbox workspace-write
praxis --sandbox danger-full-access
```

The same setting can be stored in `~/.praxis/config.toml` with
`sandbox_mode = "MODE"`.

## MCP

Praxis can act as an MCP client and connect to configured MCP servers on
startup. It can also run as an MCP server:

```shell
praxis mcp-server
```

That server mode lets other MCP clients use Praxis as an agent tool.

Use `praxis mcp` to add, list, get, remove, or authenticate MCP server launchers
defined in `config.toml`.

## Compatibility Boundaries

Praxis intentionally keeps compatibility with selected external ecosystems, but
those names should stay at the edge:

| Name | Praxis boundary |
|---|---|
| Codex | Login compatibility and external thread import source. |
| OpenAI / GPT | Provider, auth, model family, OpenAI Responses behavior profile. |
| Claude | Provider, wire, external migration source. |
| Cursor | External session migration source. |
| Cunning3D | Product profile, plugin bundle, domain prompt/tool policy. |

If a type or module is not specifically about one of those compatibility
boundaries, it should use Praxis domain language.

## Development References

Start here when changing architecture:

- [`docs/refactor/cleanroom_target_architecture.md`](./docs/refactor/cleanroom_target_architecture.md)
- [`docs/refactor/main_framework_naming.md`](./docs/refactor/main_framework_naming.md)
- [`docs/refactor/tui_workspace_state_machine_r1.md`](./docs/refactor/tui_workspace_state_machine_r1.md)

General user and configuration docs live one directory above this Rust
workspace:

- [`../docs/getting-started.md`](../docs/getting-started.md)
- [`../docs/install.md`](../docs/install.md)
- [`../docs/config.md`](../docs/config.md)
- [`../docs/app-gateway.md`](../docs/app-gateway.md)
- [`../docs/skills.md`](../docs/skills.md)

Build from this workspace when validating Praxis itself:

```shell
cargo build -p praxis-cli --bin praxis
```
