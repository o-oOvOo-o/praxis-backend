---
name: praxis-thread-control
description: Use when coordinating loaded Praxis threads through App Gateway thread APIs.
---

# Praxis Thread Control

Use this skill when operating Praxis threads from a host, Center, TUI, or future GUI surface.

## Boundaries

- Use existing thread APIs as the control plane.
- Do not duplicate ThreadManager or AgentOs logic inside plugin code.
- Treat App Gateway as transport; thread semantics stay in Praxis core.
- Acquire control before steering a thread when the caller needs read-only or exclusive interaction.

## Expected Flow

1. List or read the target thread.
2. Acquire control when needed with an explicit controller identity.
3. Resume, fork, steer, interrupt, or release through App Gateway APIs.
4. Surface control-state changes back to the caller.
