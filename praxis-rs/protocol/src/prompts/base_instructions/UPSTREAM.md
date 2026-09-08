# Upstream model prompts

Source: https://github.com/openai/codex
Commit: `12fe9f822c0b02a9a257e0a248b17f1a3ad7b941` (2026-09-05).
Source file: `codex-rs/models-manager/models.json`.

- `gpt_6.md`: `gpt-6-astra.model_messages.instructions_template`.
- `gpt_5_6.md`: `gpt-5.6-sol.model_messages.instructions_template`, identical for Terra and Luna.

Modified for Praxis: only `You are Codex,` and `As Codex,` become `You are Praxis,` and `As Praxis,`. Model names, tool identifiers, environment names, placeholders, and all other wording are preserved. See LICENSE and NOTICE for upstream attribution.

These are the standard model instructions. Conditional persistent-mode, approval, tool, and collaboration overlays remain owned by the Praxis runtime and are not appended unconditionally.
