# TypeScript Hot Flywheel

This plugin-owned runner keeps parity iteration outside Cargo while preserving the canonical provider, typed-buffer, first-mismatch, artifact, and receipt contracts.

Run every command through the canonical wrapper:

```powershell
.\crates\praxis-backend\praxis-rs\plugins\official\cunning3d-gaea-flywheel\scripts\c3d-flywheel.ps1 hot test
.\crates\praxis-backend\praxis-rs\plugins\official\cunning3d-gaea-flywheel\scripts\c3d-flywheel.ps1 hot houdini-verb-info --subject matchsize
.\crates\praxis-backend\praxis-rs\plugins\official\cunning3d-gaea-flywheel\scripts\c3d-flywheel.ps1 hot houdini-facet --matrix focused --run --json
.\crates\praxis-backend\praxis-rs\plugins\official\cunning3d-gaea-flywheel\scripts\c3d-flywheel.ps1 hot houdini-facet --matrix semantic --capture-only --run --json
.\crates\praxis-backend\praxis-rs\plugins\official\cunning3d-gaea-flywheel\scripts\c3d-flywheel.ps1 hot houdini-facet --matrix semantic --oracle <houdini_capture.json> --run --json
.\crates\praxis-backend\praxis-rs\plugins\official\cunning3d-gaea-flywheel\scripts\c3d-flywheel.ps1 hot houdini-match-size --matrix focused --run --json
.\crates\praxis-backend\praxis-rs\plugins\official\cunning3d-gaea-flywheel\scripts\c3d-flywheel.ps1 hot houdini-match-size --matrix focused --oracle <houdini_capture.json> --run --json
.\crates\praxis-backend\praxis-rs\plugins\official\cunning3d-gaea-flywheel\scripts\c3d-flywheel.ps1 hot houdini-boolean30-vertex-refmap --trace <transfer_trace.json> --actual <actual.json> --oracle <oracle.json> --case <case_id> --output <receipt.json> --run --json
.\crates\praxis-backend\praxis-rs\plugins\official\cunning3d-gaea-flywheel\scripts\c3d-flywheel.ps1 hot houdini-boolean30-coplanar-defaults --trace <transfer_trace.json> --actual <actual.json> --oracle <oracle.json> --case <case_id> --output <receipt.json> --run --json
.\crates\praxis-backend\praxis-rs\plugins\official\cunning3d-gaea-flywheel\scripts\c3d-flywheel.ps1 hot houdini-boolean30-compare --case <case_id> --receipt <receipt.json> --run --json
.\crates\praxis-backend\praxis-rs\plugins\official\cunning3d-gaea-flywheel\scripts\c3d-flywheel.ps1 hot houdini-boolean30-membership --stage <selection_stage.json> --actual <actual.json> --oracle <oracle.json> --case <case_id> --output <receipt.json> --run --json
```

The first Match Size command captures immutable Houdini 20.5 native node-network buffers. Later edits reuse that capture with `--oracle`, run the TypeScript candidate, compare topology exactly and `P` with explicit tolerances, and write `c3d.parity.receipt.v1` under `.local/gaea-flywheel/artifacts`.

The current Match Size contract is focused evidence only: explicit nonuniform bounds matching and second-input uniform X-axis scaling with center justification. Unsupported branches fail closed. Semantic, buffer-exact, and production promotion require broader matrices and the final native Cunning3D implementation.

Facet has both focused and semantic profiles. The semantic TypeScript pipeline follows Houdini's stage order and covers pre/post normals, unit and reverse normals, Unique Points, all four Consolidate modes, accurate versus legacy box distance, inline removal, polygon orientation, cusp splitting, topological degenerate removal, planarization, point/primitive group selection, numeric group ranges, combined stages, boundary values, missing normals/groups, and malformed-input failures.

The semantic oracle currently contains 38 real Houdini 20.5.584 cases. Comparison is strict for counts, primitive/vertex order, connectivity equivalence, integer buffers, numeric attributes on point/vertex/primitive/detail domains, and point/primitive groups. Floating buffers use explicit `1e-6` absolute and relative tolerances. Every semantic run executes the TypeScript candidate twice and requires deterministic parity before emitting `matrix-parity`.

Only output point labels are renumbering-invariant, because Houdini's cusp ownership can depend on hidden upstream point representative state. Point sharing, dereferenced positions, all captured attributes, and group membership remain strict.

These receipts certify a TypeScript algorithm matrix over the normalized numeric geometry schema, not a Cunning3D Geometry roundtrip or a production node. Bun and `hython` are developer flywheel dependencies only; they are not packaged into CDA files and do not become end-user runtime dependencies. Promotion waits for the stable in-app TypeScript host or a later Rust port.

The Boolean 3.0 vertex-refmap command evaluates native discrete vertex ownership without rebuilding Rust. It consumes a Rust transfer trace, the current output, and an immutable Houdini oracle case; validates source-triangle identity; applies the crossing-endpoint cyclic-head rule; and writes the first exact mismatch receipt.

The coplanar-defaults command tests Houdini's missing-sample behavior for constructed points that participate exclusively in coplanar-boundary crossings. It preserves P, defaults all other point attributes, and leaves direct source or transverse-crossing samples unchanged.

The Boolean30 compare route launches the already-built dedicated comparator only through the canonical flywheel wrapper. It preserves diagnostic environment variables and fixes the executable target to `F:\cargo-target2\Cunning3D-boolean30-compare`.

The membership command compares actual and native triangle multisets by tolerance-quantized positions and maps every missing or extra triangle back to all matching selection-stage facets. It is diagnostic only and never supplies production decisions.
