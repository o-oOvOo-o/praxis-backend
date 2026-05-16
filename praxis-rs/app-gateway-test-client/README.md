# App Gateway Test Client
Quickstart for running and hitting `praxis app-gateway`.

## Quickstart

Run from `<reporoot>/praxis-rs`.

```bash
# 1) Build debug praxis binary
cargo build -p praxis-cli --bin praxis

# 2) Start websocket app-gateway in background
cargo run -p praxis-app-gateway-test-client -- \
  --praxis-bin ./target/debug/praxis \
  serve --listen ws://127.0.0.1:4222 --kill

# 3) Call app-gateway (defaults to ws://127.0.0.1:4222)
cargo run -p praxis-app-gateway-test-client -- model-list
```

## Watching Raw Inbound Traffic

Initialize a connection, then print every inbound JSON-RPC message until you stop it with
`Ctrl+C`:

```bash
cargo run -p praxis-app-gateway-test-client -- watch
```

## Testing Thread Rejoin Behavior

Build and start an app gateway using commands above. The app-gateway log is written to `/tmp/praxis-app-gateway-test-client/app-gateway.log`

### 1) Get a thread id

Create at least one thread, then list threads:

```bash
cargo run -p praxis-app-gateway-test-client -- send-message-api "seed thread for rejoin test"
cargo run -p praxis-app-gateway-test-client -- thread-list --limit 5
```

Copy a thread id from the `thread-list` output.

### 2) Rejoin while a turn is in progress (two terminals)

Terminal A:

```bash
cargo run --bin praxis-app-gateway-test-client -- \
  resume-message-api <THREAD_ID> "respond with thorough docs on the rust core"
```

Terminal B (while Terminal A is still streaming):

```bash
cargo run --bin praxis-app-gateway-test-client -- thread-resume <THREAD_ID>
```
