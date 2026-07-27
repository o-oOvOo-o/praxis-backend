---
name: cunning3d-computer-use
description: Use when observing or interacting with Cunning3D, Metra, or Windows UI through the native computer-use runtime.
---

# Cunning3D Computer Use

Use this skill for semantic-first native UI observation and interaction.

## Tools

- `cunning3d_computer_use_observe` lists applications or windows, captures semantic snapshots, and takes screenshots.
- `cunning3d_computer_use_interact` activates, invokes, edits, focuses, clicks, types, sends keys, scrolls, or drags.
- Both tools are deferred until relevant and accept a tagged Metra `action` object.

## Workflow

1. Observe the target before interacting.
2. Prefer stable semantic handles from the latest snapshot.
3. Prefer product-native or accessibility channels over coordinates.
4. Use pointer fallback only when semantic channels are unavailable.
5. Re-observe after mutations that can invalidate a target handle.

## Permission Contract

- Read Only permits observation and rejects interaction.
- Default and Guardian permit focus, selection, toggles, scrolling, and dragging.
- Text/value input is always treated as sensitive; invoke, click, and key input are always treated as potentially external.
- `sensitive` and `external_effect` are intent metadata and can raise context for the host, but never lower the runtime risk class.
- Full Access permits every supported action without an approval prompt.
- The runtime reads the current thread permission on every invocation and never caches permission or approval state.
- On `approval_required`, do not retry the same call. Continue only after a scoped host approval or an explicit user permission change.

## Boundaries

- The runtime is host-neutral and must not depend on Harness.
- Hosts provide product-native or browser channels; the runtime composes them with Metra Windows channels.
- Safe fallback is allowed only before a channel may have mutated state.
- Do not start a standalone Harness or duplicate the Cunning3D application lifecycle.
