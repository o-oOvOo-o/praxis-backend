# Configuration

Praxis reads configuration from its resolved Praxis home directory. The
generated JSON Schema for `config.toml` lives at
`praxis-rs/core/config.schema.json`.

## Connecting to MCP servers

Praxis can connect to MCP servers configured in `~/.praxis/config.toml`.

## MCP tool approvals

Praxis stores per-tool approval overrides for custom MCP servers under
`mcp_servers` in `~/.praxis/config.toml`:

```toml
[mcp_servers.docs.tools.search]
approval_mode = "approve"
```

## Apps (Connectors)

Use `$` in the composer to insert a ChatGPT connector; the popover lists accessible
apps. The `/apps` command lists available and installed apps. Connected apps appear first
and are labeled as connected; others are marked as can be installed.

## Plugin Marketplaces

Praxis plugins use `.praxis-plugin/plugin.json` as their manifest entrypoint.
Marketplace providers are configured under `[plugin_marketplaces.<name>]`:

```toml
[plugin_marketplaces.local-dev]
provider = "local"
path = "D:/path/to/plugin-repo"

[plugin_marketplaces.cunning3d]
provider = "git"
repo = "git@github.com:cunning-org/cunning3d-praxis-plugins.git"
reference = "main"
path = "."
```

For the in-repo official development marketplace, configure a local provider whose
`path` points at the `praxis-rs` directory that contains `.agents/plugins/marketplace.json`.
Git marketplace caches are refreshed through `plugin/sync`.

Installed plugins are enabled or disabled under `[plugins."<plugin>@<marketplace>"]`:

```toml
[plugins."external-agent-migration@praxis-official"]
enabled = true
```

## Notify

Praxis can run a notification hook when the agent finishes a turn.

When Praxis knows which client started the turn, the legacy notify JSON payload also includes a top-level `client` field. The TUI reports `praxis-tui`, and the app gateway reports the `clientInfo.name` value from `initialize`.

## JSON Schema

The generated JSON Schema for `config.toml` lives at `praxis-rs/core/config.schema.json`.

## SQLite State DB

Praxis stores the SQLite-backed state DB under `sqlite_home` (config key) or the
`PRAXIS_SQLITE_HOME` environment variable. When unset, WorkspaceWrite sandbox
sessions default to a temp directory; other modes default to `PRAXIS_HOME`.

## Custom CA Certificates

Praxis can trust a custom root CA bundle for outbound HTTPS and secure websocket
connections when enterprise proxies or gateways intercept TLS. This applies to
login flows and to Praxis external connections, including components that build
reqwest clients or secure websocket clients through the
shared `praxis-client` CA-loading path and remote MCP connections that use it.

Set `CODEX_CA_CERTIFICATE` to the path of a PEM file containing one or more
certificate blocks to use the legacy compatibility CA bundle override. If
`CODEX_CA_CERTIFICATE` is unset, Praxis falls back to `SSL_CERT_FILE`. If
neither variable is set, Praxis uses the system root certificates.

`CODEX_CA_CERTIFICATE` takes precedence over `SSL_CERT_FILE`. Empty values are
treated as unset. This variable is retained for compatibility until the CA
loading path grows a Praxis-named alias.

The PEM file may contain multiple certificates. Praxis also tolerates OpenSSL
`TRUSTED CERTIFICATE` labels and ignores well-formed `X509 CRL` sections in the
same bundle. If the file is empty, unreadable, or malformed, the affected Praxis
HTTP or secure websocket connection reports a user-facing error that points
back to these environment variables.

## Notices

Praxis stores "do not show again" flags for some UI prompts under the `[notice]` table.

## Plan mode defaults

`plan_mode_reasoning_effort` lets you set a Plan-mode-specific default reasoning
effort override. When unset, Plan mode uses the built-in Plan preset default
(currently `medium`). When explicitly set (including `none`), it overrides the
Plan preset. The string value `none` means "no reasoning" (an explicit Plan
override), not "inherit the global default". There is currently no separate
config value for "follow the global default in Plan mode".

## Realtime start instructions

`experimental_realtime_start_instructions` lets you replace the built-in
developer message Praxis inserts when realtime becomes active. It only affects
the realtime start message in prompt history and does not change websocket
backend prompt settings or the realtime end/inactive message.

Ctrl+C/Ctrl+D quitting uses a ~1 second double-press hint (`ctrl + c again to quit`).
