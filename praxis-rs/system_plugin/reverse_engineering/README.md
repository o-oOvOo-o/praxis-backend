# Praxis System Plugin: Reverse Engineering

This crate is a built-in runtime subsystem, not a marketplace plugin. It owns authorized reverse-engineering primitives for Praxis Tools Runtime:

- per-target authorization scopes
- local artifact ingest and codec projection
- redaction and zero-raw-exposure invariants
- append-only evidence ledgers
- reverse toolchain registry and doctor output
- safety evaluation records

User-facing capability, prompts, and skills live in `plugins/official/praxis-reverse-engineering`.
Model-visible tools are registered by `praxis-tools` and executed through `praxis-core`.
