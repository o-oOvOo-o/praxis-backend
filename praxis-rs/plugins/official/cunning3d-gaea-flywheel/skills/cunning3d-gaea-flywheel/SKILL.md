---
name: cunning3d-gaea-flywheel
description: Run and advance the canonical Cunning3D Gaea heightfield flywheel from Praxis, including node status, reverse evidence, focused exact parity, mandatory ledger writeback, blocker memory, hygiene gates, and `/gaeawheel` observability after every meaningful node attempt.
---

# Cunning3D Gaea Flywheel

Use this skill when the user asks Praxis to inspect, run, debug, or advance a Gaea heightfield node through the Cunning3D flywheel.

## Canonical execution contract

- Treat the plugin's `runtime/c3d_devflywheeltool` directory as the only flywheel business, ledger, and evidence implementation.
- Run commands through `/gaea <command>` in Praxis. For plugin development, use `scripts/c3d-flywheel.ps1 <command>` from this plugin root.
- Never reproduce ledger scoring, parity classification, next-command selection, or artifact promotion in plugin or TUI code.
- Use `probe-bin --bin gaea_<probe> -- <args>` when no first-class flywheel command exists.
- Use the canonical Gaea target directory selected by the wrapper; never create task-specific Cargo target roots.

## Workflow

1. Run `ledger-hygiene --json --strict` before ledger or graph edits.
2. Inspect `status --node <Node> --json`, `open-frontier --node <Node> --all --json`, and `reverse --node <Node> --json`.
3. Establish the smallest fresh Bridge-vs-Native failing case and localize the first mismatching stage or scalar.
4. Fix shared substrate before recipe-specific symptoms when evidence shows a reusable contract gap.
5. Run the focused exact gate first. Do not expand to heavy matrices unless requested or needed for promotion.
6. Perform the mandatory writeback below for both success and useful failure.
7. Run the observability gate below before reporting completion.

## Mandatory writeback

- Treat artifact generation without ledger writeback as incomplete work.
- Update the plugin-owned `runtime/c3d_devflywheeltool/ledger/gaea_operator_ledger.json`; never create a side ledger.
- For every affected contract entry, update `status`, `native_evidence`, `rust_implementation`, `evidence_summary`, and `open_risk` from fresh evidence.
- Use `audited_closed` only for the documented full exact contract. Use `focused_closed` only for explicitly bounded focused closure. Otherwise keep the entry open and record the exact remaining mismatch, rejected hypothesis, artifact path, and next focused command.
- Write back useful failed attempts too. A disproved formula, localized first mismatch, or newly isolated blocker is flywheel memory and must prevent the next agent from repeating the work.
- Update `gaea_flywheel_graph.json` when operator dependencies, blockers, or node relationships change. Update the acceptance matrix when the promotion contract changes.
- Never claim completion from code, screenshots, chat notes, or an artifact directory alone.

## Observability gate

Run all applicable commands after writeback:

```text
/gaea ledger-hygiene --json --strict
/gaea status --node <Node> --json
/gaea verify --node <Node> --json
/gaea praxis-panel --node <Node> --json
```

- Confirm `/gaeawheel` shows the new progress/status and blocker summary from the same canonical ledger.
- If meaningful work occurred but the ledger-backed panel did not change, the flywheel turn is not complete.
- Report the before/after ledger status, fresh artifact path, remaining blocker, and the `/gaea` command that reproduces the next step.

## Praxis commands

- `/gaeawheel` renders the canonical CLI-owned flywheel panel.
- `/gaea <command> <args>` invokes the plugin-owned real flywheel CLI.

Do not claim a node is closed unless strict raw parity, required branch coverage, ledger contract status, `/gaeawheel` projection, and Loom/performance gates required by that node all agree.
