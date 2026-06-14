# Praxis App Gateway Destructive Refactor

## Position

`app_gateway` is the public application boundary for Praxis. `server` is only one deployment mode.

The final architecture has one canonical gateway core and two transport families:

- Native mode: in-process, typed, low-latency integration for desktop apps and native hosts.
- Service mode: out-of-process JSON-RPC transport for IDEs, web clients, remote clients, and third-party tools.

Both modes must call the same dispatcher and share the same protocol vocabulary. Transport code may own connection setup, framing, auth handshakes, and event delivery, but it must not fork business behavior.

## Public Crates

```text
praxis-app-gateway-protocol
  wire and host capability vocabulary

praxis-app-gateway-core
  canonical dispatch context, dispatcher trait, event sink, host registry

praxis-app-gateway-native
  in-process handle for desktop and native hosts; owns the public native runtime entry point

praxis-app-gateway-service
  process/socket transport boundary; owns the public service runtime entry point

praxis-app-gateway-client
  client-side request abstraction

praxis-app-gateway
  shared runtime and message processor used by native and service modes

praxis-host-sdk
  generic host extension traits

praxis-metra-gateway
  official Metra bridge for semantic UI observation and native command routing
```

The previous server-named crates have been retired from the workspace; new work should use the App Gateway crates directly.

## Core Rules

- Public Praxis backend code must stay product-neutral.
- Host-specific behavior enters through `praxis-host-sdk`.
- Metra is a first-class official desktop bridge, not an app-specific shortcut.
- App Gateway sessions use `SessionSource::AppGateway`; they are not folded into MCP or editor sessions.
- Native mode must avoid hot-path JSON serialization and process hops.
- Service mode must preserve the remote/client capability envelope.
- Event delivery is lane-based: realtime, snapshot, background, telemetry.
- Capability negotiation must happen at gateway initialization.
- Product adapters may register host panels, commands, semantic surfaces, and tools, but those adapters live outside the public Praxis backend.

## Performance Contract

Native mode exists to make desktop feedback feel immediate:

- request dispatch stays in process;
- event queues are typed;
- snapshots are explicit;
- UI semantic state is observable through Metra descriptors;
- command routing uses host-registered capabilities instead of stringly product RPCs.

Service mode remains the compatibility and remote access path. It should be complete, but not the lowest-latency desktop path.

## Final Route

1. Keep all public APIs under `app-gateway-*`, `host-sdk`, and `metra-gateway`.
2. Keep protocol vocabulary in `praxis-app-gateway-protocol`.
3. Keep request and event boundaries in `praxis-app-gateway-core`.
4. Route embedded desktop/TUI/exec users through `praxis-app-gateway-native`.
5. Route external process/socket users through `praxis-app-gateway-service`.
6. Do not reintroduce public server naming.

This is intentionally a breaking route. Long-term compatibility facades should not be added unless release policy explicitly requires them.
