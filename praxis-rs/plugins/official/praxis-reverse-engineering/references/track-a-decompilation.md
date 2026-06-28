# Track A: Authorized Analysis

Track A analyzes targets the user owns, is licensed to inspect, or is allowed to test in a lab. The goal is evidence, interface mapping, parity, and clean-room reconstruction.

Required flow:

1. Call `reverse_authorize_target` and capture scope id, target hash, target kind, note, allowed actions, and forbidden actions.
2. Run `reverse_toolchain_status` and choose a concrete backend from the registry.
3. Use `reverse_target_fingerprint` and `reverse_artifact_ingest` before any invasive analyzer.
4. Use `reverse_artifact_summarize`, `reverse_artifact_redact`, `reverse_compare_behavior`, and `reverse_record_evidence` for the current registered tool surface.
5. Treat `reverse_extract_static`, `reverse_shader_reflect`, `reverse_decompile_function`, and `reverse_probe_blackbox` as adapter-phase tools until they are registered by the runtime.
6. Send the model codec projections, not raw analyzer output.
7. Record facts with `reverse_record_evidence` and validate claims with `reverse_compare_behavior`.

Allowed outputs:

- hashes, symbols, imports, exports, metadata, callgraph summaries, CFG summaries, behavior summaries, fixture ids, and bounded clean-room pseudocode
- clean-room implementation plans that cite evidence records

Disallowed outputs:

- payload text, exploit chains, credential extraction, stealth, persistence, DRM bypass, activation bypass, anti-cheat bypass, or copied proprietary source
