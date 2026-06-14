---
name: external-agent-migration
description: Use when importing, inspecting, or forking external agent state from Codex, Claude, or Cursor into Praxis.
---

# External Agent Migration

Use this skill when a task involves external agent state migration.

## Boundaries

- Treat Codex, Claude, and Cursor as providers behind one migration model.
- Discover sessions and configuration without mutating source agent homes.
- Fork/import into Praxis through the thread store and App Gateway thread APIs.
- Keep provider-specific parsing under provider modules, not in TUI or Gateway code.

## Expected Flow

1. Detect available provider homes and workspace-specific stores.
2. List candidate sessions with cwd, title, last activity, and source path.
3. Convert provider transcript items into Praxis thread history.
4. Fork or import through Praxis thread creation/resume APIs.
5. Report skipped items with explicit diagnostics instead of silently dropping them.
