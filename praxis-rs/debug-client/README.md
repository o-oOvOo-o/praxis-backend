WARNING: this code is experimental and should not be used in production

# praxis-debug-client

A tiny interactive client for `praxis app-gateway` canonical protocol. It prints
all JSON-RPC lines from the server and lets you send new turns as you type.

## Usage

Start the app-gateway client (it will spawn `praxis app-gateway` itself):

```
cargo run -p praxis-debug-client -- \
  --praxis-bin praxis \
  --approval-policy on-request
```

You can resume a specific thread:

```
cargo run -p praxis-debug-client -- --thread-id thr_123
```

### CLI flags

- `--praxis-bin <path>`: path to the `praxis` binary (default: `praxis`).
- `-c, --config key=value`: pass through `--config` overrides to `praxis`.
- `--thread-id <id>`: resume a thread instead of starting a new one.
- `--approval-policy <policy>`: `untrusted`, `on-failure` (deprecated), `on-request`, `never`.
- `--auto-approve`: auto-approve command/file-change approvals (default: decline).
- `--final-only`: only show completed assistant messages and tool items.
- `--model <name>`: optional model override for thread start/resume.
- `--model-provider <name>`: optional provider override.
- `--cwd <path>`: optional working directory override.

## Interactive commands

Type a line to send it as a new turn. Commands are prefixed with `:`:

- `:help` show help
- `:new` start a new thread
- `:resume <thread-id>` resume a thread
- `:use <thread-id>` switch active thread without resuming
- `:refresh-thread` list available threads
- `:quit` exit

The prompt shows the active thread id. Client messages (help, errors, approvals)
print to stderr; raw server JSON prints to stdout so you can pipe/record it
unless `--final-only` is set.

## Notes

- The client performs the required initialize/initialized handshake.
- It prints every server notification and response line as it arrives.
- Approvals for `item/commandExecution/requestApproval` and
  `item/fileChange/requestApproval` are auto-responded to with decline unless
  `--auto-approve` is set.