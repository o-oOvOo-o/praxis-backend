# Praxis Reverse Engineering Architecture

## Decision

`praxis-reverse-engineering` is a Praxis plugin/product mode backed by a built-in runtime subsystem. It should not modify core agent loops and should not rely on prompt-only behavior for powerful analysis. Prompts and skills define the mode; the built-in runtime and Tools Runtime execute decompiler work under feature flags, approval, sandbox, artifact codec, and output policy.

## Ownership

- L1 plugin package: manifest, skills, product prompt, workflow policy, and user-facing capability metadata.
- L2 built-in runtime crate: authorization scopes, artifact store, codec projection, redaction, toolchain doctor, ledger, parity, and safety eval records.
- L3 Tools Runtime: model-visible tool specs, approval requests, handler dispatch, sandbox handoff, and model output shaping.
- MCP server: optional external transport for decompiler tools when the implementation should be independently restartable.
- C3D Gaea flywheel: product-specific reverse evidence, oracle probes, ledgers, and parity acceptance.
- Harness/Cunning3D GUI: controls, artifact viewers, diff/evidence panels, and workflow status.

## First Native Tool Set

The first registered tool set avoids "run arbitrary decompiler command" as the public API. Use structured verbs:

- `reverse_authorize_target`: ask the user to authorize one local target and create a scoped consent record.
- `reverse_revoke_target`: revoke a scoped consent record.
- `reverse_toolchain_status`: inspect configured decompiler/debugger/runtime availability without running analyzers.
- `reverse_target_fingerprint`: hash and classify a local target.
- `reverse_artifact_ingest`: hash, classify, scope, and store raw reverse-engineering artifacts without sending raw content to the model.
- `reverse_artifact_summarize`: produce safe model-facing projections from local raw artifacts.
- `reverse_artifact_redact`: remove or bucket payload-like literals, credentials, raw byte arrays, suspicious strings, and proprietary code before transcript exposure.
- `reverse_compare_behavior`: compare observed behavior against clean-room implementation output.
- `reverse_record_evidence`: append structured evidence to a scoped ledger.
- `reverse_safety_eval_plan`: create a scoped plan around opaque safety fixture ids.
- `reverse_safety_eval_run_fixture`: record an approved local fixture run request without exposing fixture contents.
- `reverse_safety_eval_record_result`: append a neutral safety evaluation result.

Analyzer tools stay adapter-phase and are not registered until their backend adapters are wired:

- `reverse_extract_static`: exports/imports/strings/metadata/resources/sections.
- `reverse_decompile_function`: bounded function/class/method extraction by symbol or address.
- `reverse_shader_reflect`: DXIL/SPIR-V/bytecode reflection and resource binding extraction.
- `reverse_probe_blackbox`: controlled execution/probe with explicit input corpus and timeout.

## Safety Model

Every mutating or invasive tool request should carry:

- `scope_id`
- `target_path`
- `authorization_note`
- `allowed_actions`
- `forbidden_actions`
- `artifact_root`
- `max_output_bytes`

The tool layer should fail closed when authorization or target scope is missing.

## Artifact Codec Boundary

Tokenizer or content filters can misclassify legitimate reverse-engineering material because raw artifacts often contain payload-looking bytes, suspicious API names, obfuscated identifiers, exploit strings, or decompiler fragments. Praxis should not try to bypass those filters. It should avoid exposing raw material to the language model unless necessary.

Data path:

```text
raw binary / decompiler output / fixture
  -> scoped artifact store
  -> local analyzer
  -> redaction and bucketing
  -> model-facing evidence JSON
  -> clean-room reasoning and task planning
```

Model-facing evidence should prefer:

- artifact ids, hashes, and provenance
- symbols, exports, imports, sections, resources, and metadata
- callgraph and control-flow summaries
- behavior labels and feature vectors
- bounded clean-room pseudocode
- references to local artifact paths for approved tools

This preserves legitimate capability while reducing accidental policy collisions and proprietary source leakage.

Authorization levels:

- `scoped_analysis`: local artifacts plus static analysis actions.
- `full_decompilation`: local decompiler/debugger artifacts may be produced under the scope, but model exposure remains `codec_projection_only`.
- `owned_hardening`: owned-target hardening plus Track A inspection, also `codec_projection_only`.

The invariant is non-negotiable: approval changes local raw access, not model-visible raw-token exposure.

## Safety Red-Team Track

The red-team track exists to improve Praxis safety, not to create an evasion product. It should support:

- high-level failure taxonomies
- opaque adversarial fixture ids
- fixture hashing and provenance
- normal model/tool approval paths
- aggregate pass/fail/severity reports
- remediation tasks for prompt, tool, sandbox, approval, and UI layers

It should not support:

- generating bypass prompts
- optimizing wording against a live model
- extracting hidden prompts
- disabling policy checks
- exposing blocked fixture bodies in ordinary chat output

## Track Documents

- Track A authorized analysis: `references/track-a-decompilation.md`
- Track B owned hardening: `references/track-b-hardening.md`
