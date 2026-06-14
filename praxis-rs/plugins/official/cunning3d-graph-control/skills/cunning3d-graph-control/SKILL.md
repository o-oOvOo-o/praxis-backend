---
name: cunning3d-graph-control
description: Use when controlling Cunning3D graphs, nodes, parameters, display flags, or product diagnostics from Praxis.
---

# Cunning3D Graph Control

Use this skill when the task needs Cunning3D product graph control.

## Boundaries

- Product graph behavior belongs to Cunning3D/cunning_core.
- Praxis plugin code should declare and route capability; it should not own graph semantics.
- UI automation belongs to Metra, not this plugin.
- MCP or app adapters must stay thin over the canonical product bridge.

## Expected Flow

1. Resolve the active Cunning3D bridge or product session.
2. Inspect current graph state before mutating it.
3. Apply node, connection, parameter, or display-flag operations through the product bridge.
4. Report diagnostics and resulting graph state.
