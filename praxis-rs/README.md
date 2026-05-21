# Praxis CLI (Rust Implementation)

We provide Praxis CLI as a standalone, native executable to ensure a zero-dependency install.

## Installing Praxis

Today, the easiest way to install Praxis is via `npm`:

```shell
npm i -g @openai/praxis
praxis
```

You can also install via Homebrew (`brew install --cask praxis`) or download a platform-specific release directly from our [GitHub Releases](https://github.com/o-oOvOo-o/praxis-backend/releases).

## Documentation quickstart

- First run with Praxis? Start with [`docs/getting-started.md`](../docs/getting-started.md) (links to the walkthrough for prompts, keyboard shortcuts, and session management).
- Want deeper control? See [`docs/config.md`](../docs/config.md) and [`docs/install.md`](../docs/install.md).

## What's new in the Rust CLI

The Rust implementation is now the maintained Praxis CLI and serves as the default experience. It includes a number of features that the legacy TypeScript CLI never supported.

### Config

Praxis supports a rich set of configuration options. Note that the Rust CLI uses `config.toml` instead of `config.json`. See [`docs/config.md`](../docs/config.md) for details.

### Model Context Protocol Support

#### MCP client

Praxis CLI functions as an MCP client that allows the Praxis CLI and IDE extension to connect to MCP servers on startup. See the [`configuration documentation`](../docs/config.md#connecting-to-mcp-servers) for details.

#### MCP server (experimental)

Praxis can be launched as an MCP _server_ by running `praxis mcp-server`. This allows _other_ MCP clients to use Codex as a tool for another agent.

Use the [`@modelcontextprotocol/inspector`](https://github.com/modelcontextprotocol/inspector) to try it out:

```shell
npx @modelcontextprotocol/inspector praxis mcp-server
```

Use `praxis mcp` to add/list/get/remove MCP server launchers defined in `config.toml`, and `praxis mcp-server` to run the MCP server directly.

### Notifications

You can enable notifications by configuring a script that is run whenever the agent finishes a turn. The [notify documentation](../docs/config.md#notify) includes a detailed example that explains how to get desktop notifications via [terminal-notifier](https://github.com/julienXX/terminal-notifier) on macOS. When Praxis detects that it is running under WSL 2 inside Windows Terminal (`WT_SESSION` is set), the TUI automatically falls back to native Windows toast notifications so approval prompts and completed turns surface even though Windows Terminal does not implement OSC 9.

### `praxis exec` to run Praxis programmatically/non-interactively

To run Codex non-interactively, run `praxis exec PROMPT` (you can also pass the prompt via `stdin`) and Praxis will work on your task until it decides that it is done and exits. If you provide both a prompt argument and piped stdin, Codex appends stdin as a `<stdin>` block after the prompt so patterns like `echo "my output" | praxis exec "Summarize this concisely"` work naturally. Output is printed to the terminal directly. You can set the `RUST_LOG` environment variable to see more about what's going on.
Use `praxis exec --ephemeral ...` to run without persisting session rollout files to disk.

### Experimenting with the Praxis Sandbox

To test to see what happens when a command is run under the sandbox provided by Praxis, we provide the following subcommands in Praxis CLI:

```
# macOS
praxis sandbox macos [--full-auto] [--log-denials] [COMMAND]...

# Linux
praxis sandbox linux [--full-auto] [COMMAND]...

# Windows
praxis sandbox windows [--full-auto] [COMMAND]...

# Legacy aliases
praxis debug seatbelt [--full-auto] [--log-denials] [COMMAND]...
praxis debug landlock [--full-auto] [COMMAND]...
```

### Selecting a sandbox policy via `--sandbox`

The Rust CLI exposes a dedicated `--sandbox` (`-s`) flag that lets you pick the sandbox policy **without** having to reach for the generic `-c/--config` option:

```shell
# Run Praxis with the default, read-only sandbox
praxis --sandbox read-only

# Allow the agent to write within the current workspace while still blocking network access
praxis --sandbox workspace-write

# Danger! Disable sandboxing entirely (only do this if you are already running in a container or other isolated env)
praxis --sandbox danger-full-access
```

The same setting can be persisted in `~/.praxis/config.toml` via the top-level `sandbox_mode = "MODE"` key, e.g. `sandbox_mode = "workspace-write"`.
In `workspace-write`, Praxis also includes `~/.praxis/memories` in its writable roots so memory maintenance does not require an extra approval.

## Code Organization

This folder is the root of a Cargo workspace. It contains quite a bit of experimental code, but here are the key crates:

- [`core/`](./core) contains the business logic for Praxis. Ultimately, we hope this to be a library crate that is generally useful for building other Rust/native applications that use Praxis.
- [`exec/`](./exec) "headless" CLI for use in automation.
- [`tui/`](./tui) CLI that launches a fullscreen TUI built with [Ratatui](https://ratatui.rs/).
- [`cli/`](./cli) CLI multitool that provides the aforementioned CLIs via subcommands.

If you want to contribute or inspect behavior in detail, start by reading the module-level `README.md` files under each crate and run the project workspace from the top-level `praxis-rs` directory so shared config, features, and build scripts stay aligned.
