## Sandbox & approvals

Praxis sandboxing and approvals are owned by the tools runtime.

The active permission profile determines which filesystem and network actions
can run without an approval prompt.

Sandbox selection resolves isolation requirements against the enabled platform
backend. `Require` always requires a backend. `Auto` requires one when the
filesystem policy, network policy, or managed network requirements demand local
isolation. If no backend is enabled or supported, selection returns
`SandboxSelectionError::RequiredSandboxUnavailable` before execution. This also
applies to Windows when its sandbox is disabled but local isolation is required.

Unrestricted execution and externally enforced policies can select no local
sandbox when no local isolation is required. Explicit sandbox bypass remains an
approval-controlled execution path. A selection error is a configuration or
platform capability error, not a sandbox denial eligible for unsandboxed retry.

The sandboxing crate owns Windows filesystem policy validation and the
restricted-token deny-write overlay. Exec consumes that validated overlay;
platform policy compatibility decisions and their regression tests live with
the sandbox implementation.
