use serde::Serialize;
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};

use super::{Context, NODE_PERFORMANCE_ACCEPTANCE_MATRIX_PATH, TOOL_COMMAND};

const SCHEMA_VERSION: u32 = 9;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum GuardVerdict {
    Pass,
    Warn,
    Fail,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum CheckSeverity {
    Pass,
    Warning,
    Blocker,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum CheckStatus {
    Pass,
    Warn,
    Fail,
    Missing,
}

#[derive(Debug, Clone, Serialize)]
struct SourceSpan {
    path: String,
    line_number: usize,
    line: String,
}

#[derive(Debug, Clone, Serialize)]
struct ArchitectureCheck {
    id: &'static str,
    title: &'static str,
    severity: CheckSeverity,
    status: CheckStatus,
    message: String,
    evidence_refs: Vec<String>,
    source_spans: Vec<SourceSpan>,
    suggested_fix: String,
    metra_card_kind: &'static str,
}

#[derive(Debug, Clone, Serialize)]
struct ArchitectureGuiSection {
    id: &'static str,
    title: &'static str,
    card_kind: &'static str,
    check_ids: Vec<&'static str>,
}

#[derive(Debug, Clone, Serialize)]
struct ArchitectureGuardReport {
    schema_version: u32,
    command: &'static str,
    node: String,
    scope: &'static str,
    verdict: GuardVerdict,
    blocking_count: usize,
    warning_count: usize,
    checks: Vec<ArchitectureCheck>,
    gui_sections: Vec<ArchitectureGuiSection>,
    next_commands: Vec<String>,
    truth_rule: &'static str,
}

#[derive(Debug, Clone)]
struct NodeSource {
    path: PathBuf,
    text: String,
    lines: Vec<String>,
}

#[derive(Debug, Clone)]
struct GuardInputs {
    repo_dir: PathBuf,
    node: String,
    node_source: Option<NodeSource>,
    substrate_source: Option<NodeSource>,
    node_definition_source: Option<NodeSource>,
    cce_planner_source: Option<NodeSource>,
    decompiled_source: Option<NodeSource>,
    acceptance_matrix: Option<Value>,
}

pub(crate) fn command_payload(ctx: &Context, node: &str) -> Result<Value, String> {
    guard_payload(ctx, node)
}

pub(crate) fn cmd_architecture_guard(ctx: &Context, cli: &super::Cli) -> Result<(), String> {
    let payload = command_payload(ctx, &cli.node())?;
    let blocked = has_blockers(&payload);
    super::print_value(cli.json(), &payload);
    if cli.has("strict") && blocked {
        return Err("CCE architecture guard rejected node promotion.".to_string());
    }
    Ok(())
}

pub(crate) fn guard_payload(ctx: &Context, node: &str) -> Result<Value, String> {
    let inputs = GuardInputs::load(ctx, node);
    let mut checks = Vec::new();
    checks.push(check_node_wrapper_shape(&inputs));
    checks.push(check_substrate_placement(&inputs));
    checks.push(check_surface_contract(&inputs));
    checks.push(check_node_catalog_loom_publication(&inputs));
    checks.push(check_residency_path(&inputs));
    checks.push(check_materialization_path(&inputs));
    checks.push(check_cce_product_authority(&inputs));
    checks.push(check_no_node_local_gpu_authority(&inputs));
    checks.push(check_closed_world_node_gpu_authority(&inputs));
    checks.push(check_no_node_specific_runtime_projection_authority(&inputs));
    checks.push(check_no_runtime_parameter_packer_framework(&inputs));
    checks.push(check_canonical_shader_authority(&inputs));
    checks.push(check_performance_claims(&inputs));

    let blocking_count = checks
        .iter()
        .filter(|check| check.severity == CheckSeverity::Blocker)
        .count();
    let warning_count = checks
        .iter()
        .filter(|check| check.severity == CheckSeverity::Warning)
        .count();
    let verdict = if blocking_count > 0 {
        GuardVerdict::Fail
    } else if warning_count > 0 {
        GuardVerdict::Warn
    } else {
        GuardVerdict::Pass
    };

    let report = ArchitectureGuardReport {
        schema_version: SCHEMA_VERSION,
        command: "architecture-guard",
        node: node.to_string(),
        scope: "gaea_heightfield_art",
        verdict,
        blocking_count,
        warning_count,
        checks,
        gui_sections: gui_sections(),
        next_commands: next_commands(node),
        truth_rule: "Skill guidance is advisory; this Rust CLI report is the promotion gate consumed by strict verify, certify, direct architecture checks, and future Metra UI cards.",
    };
    serde_json::to_value(report)
        .map_err(|error| format!("Failed to serialize guard report: {error}"))
}

pub(crate) fn has_blockers(payload: &Value) -> bool {
    payload
        .get("blocking_count")
        .and_then(Value::as_u64)
        .unwrap_or(0)
        > 0
}

fn check_node_wrapper_shape(inputs: &GuardInputs) -> ArchitectureCheck {
    let Some(source) = &inputs.node_source else {
        return check(
            "node_wrapper_shape",
            "HeightField node wrapper exists",
            CheckSeverity::Blocker,
            CheckStatus::Missing,
            format!(
                "No Rust node wrapper was found for '{}'. A Gaea HeightField Art node cannot be promoted without a visible wrapper boundary.",
                inputs.node
            ),
            vec![],
            vec![],
            "Create or locate the node wrapper under src/nodes/heightfield, then keep it limited to params, ports, typed settings, and substrate calls.",
            "source_blocker",
        );
    };

    let algorithm_hits = line_hits(
        source,
        &[
            "for y in",
            "for x in",
            ".iter_mut().enumerate()",
            ".chunks_mut(",
            "height_mut",
            "samples_mut",
            "Vec::<f32>",
            "while ",
        ],
        12,
    );
    let materialization_hits = line_hits(
        source,
        &[
            "read_cpu_full_blocking(",
            "recover_heightfield_from_geometry_blocking(",
        ],
        12,
    );
    let line_count = source.lines.len();
    let severity =
        if !materialization_hits.is_empty() || (line_count > 800 && algorithm_hits.len() >= 4) {
            CheckSeverity::Blocker
        } else if line_count > 500 || !algorithm_hits.is_empty() {
            CheckSeverity::Warning
        } else {
            CheckSeverity::Pass
        };
    let status = match severity {
        CheckSeverity::Pass => CheckStatus::Pass,
        CheckSeverity::Warning => CheckStatus::Warn,
        CheckSeverity::Blocker => CheckStatus::Fail,
    };
    let mut spans = algorithm_hits;
    spans.extend(materialization_hits);
    check(
        "node_wrapper_shape",
        "Node wrapper stays thin",
        severity,
        status,
        format!(
            "Wrapper '{}' has {line_count} lines. Thin wrappers should read parameters, compose typed settings, and call substrate/runtime helpers; algorithm loops or direct materialization belong below the node boundary.",
            relative_path(&inputs.repo_dir, &source.path)
        ),
        vec![relative_path(&inputs.repo_dir, &source.path)],
        spans,
        "Move reusable math, sampling, map traversal, and materialization-heavy work into geometry/heightfield substrate or Loom runtime helpers.",
        "source_guard",
    )
}

fn check_substrate_placement(inputs: &GuardInputs) -> ArchitectureCheck {
    if let Some(source) = &inputs.substrate_source {
        return check(
            "substrate_placement",
            "Reusable algorithm lives below node shell",
            CheckSeverity::Pass,
            CheckStatus::Pass,
            format!(
                "Found a matching heightfield substrate file at '{}'.",
                relative_path(&inputs.repo_dir, &source.path)
            ),
            vec![relative_path(&inputs.repo_dir, &source.path)],
            vec![],
            "Keep reusable operators in cunning_core/core/geometry/heightfield or shared algorithms; node code should remain a shell.",
            "substrate",
        );
    }

    let spans = inputs
        .node_source
        .as_ref()
        .map(|source| {
            line_hits(
                source,
                &[
                    "fn compute",
                    "fn build",
                    "for y in",
                    "height_mut",
                    "samples_mut",
                    "Vec::<f32>",
                ],
                10,
            )
        })
        .unwrap_or_default();
    let severity = if spans.is_empty() {
        CheckSeverity::Warning
    } else {
        CheckSeverity::Blocker
    };
    let status = if spans.is_empty() {
        CheckStatus::Warn
    } else {
        CheckStatus::Fail
    };
    check(
        "substrate_placement",
        "Substrate ownership is discoverable",
        severity,
        status,
        format!(
            "No matching substrate file was found for '{}'. If this node has reusable math, the flywheel cannot verify that it is below the wrapper.",
            inputs.node
        ),
        vec![],
        spans,
        "Create or reuse a substrate module under src/cunning_core/core/geometry/heightfield and make the node wrapper call that API.",
        "substrate_blocker",
    )
}

fn check_surface_contract(inputs: &GuardInputs) -> ArchitectureCheck {
    let Some(source) = &inputs.decompiled_source else {
        return check(
            "surface_contract",
            "Gaea parameter and port surface has source evidence",
            CheckSeverity::Blocker,
            CheckStatus::Missing,
            format!(
                "No decompiled Gaea source was found for '{}'. Raw-buffer parity cannot close the node surface contract alone.",
                inputs.node
            ),
            vec![],
            vec![],
            "Run reverse --node <Node> --json and wire the recovered node_surface_contract into the ledger before claiming full closure.",
            "surface_contract",
        );
    };
    let spans = line_hits(
        source,
        &[
            "[Parameter",
            "[CanCreatePorts(",
            "base.Ports.Add",
            "AddNewPort",
            "PortCount",
            "base.Ins",
        ],
        16,
    );
    check(
        "surface_contract",
        "Surface contract is backed by decompiled source",
        CheckSeverity::Pass,
        CheckStatus::Pass,
        format!(
            "Decompiler source '{}' is available for parameter and port parity.",
            relative_path(&inputs.repo_dir, &source.path)
        ),
        vec![source.path.display().to_string()],
        spans,
        "Keep surface-contract parity separate from raw-buffer parity until parameters, defaults, visibility, ports, and dynamic limits are audited.",
        "surface_contract",
    )
}

fn check_node_catalog_loom_publication(inputs: &GuardInputs) -> ArchitectureCheck {
    let Some(source) = &inputs.node_definition_source else {
        return check(
            "node_catalog_loom_publication",
            "NodeCatalog publishes one canonical CCE/Loom surface",
            CheckSeverity::Blocker,
            CheckStatus::Missing,
            format!(
                "No canonical NodeCatalog definition was found for '{}'; wrapper registration cannot prove CCE or Loom eligibility.",
                inputs.node
            ),
            vec![],
            vec![],
            "Add one canonical node definition under crates/cunning_core/src/node_definitions with NodeCatalogEntry and NodePublication { loom: true, ... }.",
            "node_catalog_authority",
        );
    };

    let catalog_spans = line_hits(source, &["NodeCatalogEntry", "NodePublication"], 16);
    let loom_spans = line_hits(source, &["loom: true"], 8);
    if !catalog_spans.is_empty() && !loom_spans.is_empty() {
        let mut spans = catalog_spans;
        spans.extend(loom_spans);
        return check(
            "node_catalog_loom_publication",
            "NodeCatalog publishes one canonical CCE/Loom surface",
            CheckSeverity::Pass,
            CheckStatus::Pass,
            "The canonical NodeCatalog entry exposes this node to the shared CCE/Loom planner through NodePublication.loom; this flag is eligibility metadata, not a second runtime authority.".to_string(),
            vec![relative_path(&inputs.repo_dir, &source.path)],
            spans,
            "Keep editor, CDA, Loom, Praxis, and execution metadata on this single NodeCatalog authority.",
            "node_catalog_authority",
        );
    }

    let mut spans = catalog_spans;
    spans.extend(line_hits(source, &["loom: false"], 8));
    check(
        "node_catalog_loom_publication",
        "NodeCatalog publishes one canonical CCE/Loom surface",
        CheckSeverity::Blocker,
        CheckStatus::Fail,
        format!(
            "The canonical definition for '{}' does not prove both NodeCatalogEntry ownership and NodePublication.loom = true.",
            inputs.node
        ),
        vec![relative_path(&inputs.repo_dir, &source.path)],
        spans,
        "Publish the node once through NodeCatalog and set NodePublication.loom explicitly; do not restore a HeightField-specific registry.",
        "node_catalog_authority",
    )
}

fn check_residency_path(inputs: &GuardInputs) -> ArchitectureCheck {
    let mut spans = Vec::new();
    if let Some(source) = &inputs.node_source {
        spans.extend(line_hits(
            source,
            &[
                "HeightFieldHandle",
                "HeightFieldHandleResidency",
                "try_recover_heightfield_handle",
                "normalized",
                "HeightFieldGaeaArtDomain",
            ],
            12,
        ));
    }
    if let Some(source) = &inputs.substrate_source {
        spans.extend(line_hits(
            source,
            &[
                "HeightFieldHandle",
                "HeightFieldHandleResidency",
                "HeightFieldMap",
                "HeightFieldGaeaArtDomain",
                "normalized",
            ],
            12,
        ));
    }
    if spans.is_empty() {
        return check(
            "residency_path",
            "Resident handle or normalized-map path is visible",
            CheckSeverity::Warning,
            CheckStatus::Warn,
            format!(
                "No resident handle, normalized-map, or Gaea Art domain path was detected for '{}'. This can still be a source node, but promotion needs explicit residency evidence.",
                inputs.node
            ),
            vec![],
            vec![],
            "Expose the primary height/map path through HeightFieldHandle, normalized map, or equivalent runtime value before convenience CPU recovery.",
            "residency",
        );
    }
    check(
        "residency_path",
        "Resident handle or normalized-map path is visible",
        CheckSeverity::Pass,
        CheckStatus::Pass,
        "A resident handle, normalized-map, or ArtDomain path is visible in the node/substrate source.".to_string(),
        vec![],
        spans,
        "Preserve domain metadata and prefer runtime maps/handles before full HeightField recovery in connected paths.",
        "residency",
    )
}

fn check_materialization_path(inputs: &GuardInputs) -> ArchitectureCheck {
    let spans = inputs
        .node_source
        .as_ref()
        .map(|source| {
            line_hits(
                source,
                &[
                    "read_cpu_full_blocking(",
                    "recover_heightfield_from_geometry_blocking(",
                    "compute_output_ref(",
                    "compute_output_ref_inner(",
                ],
                16,
            )
        })
        .unwrap_or_default();
    if spans.is_empty() {
        return check(
            "materialization_path",
            "No hidden CPU materialization in wrapper",
            CheckSeverity::Pass,
            CheckStatus::Pass,
            "The node wrapper does not contain direct full CPU readback or geometry recovery calls.".to_string(),
            vec![],
            vec![],
            "Keep materialization explicit and telemetry-backed when it is semantically required.",
            "materialization",
        );
    }
    check(
        "materialization_path",
        "No hidden CPU materialization in wrapper",
        CheckSeverity::Blocker,
        CheckStatus::Fail,
        "The node wrapper contains direct materialization/recovery calls. Promotion must prove these are semantic barriers, not convenience fallback.".to_string(),
        vec![],
        spans,
        "Move primary connected-input recovery to resident map/handle helpers or record an explicit full-field semantic exception with raw parity evidence.",
        "materialization_blocker",
    )
}

fn check_cce_product_authority(inputs: &GuardInputs) -> ArchitectureCheck {
    let Some(node_definition) = &inputs.node_definition_source else {
        return check(
            "cce_product_authority",
            "EngineHosted execution has one formal authority",
            CheckSeverity::Blocker,
            CheckStatus::Missing,
            format!(
                "No canonical NodeProduct definition was found for '{}'; a node wrapper or parity artifact alone cannot enter CCE.",
                inputs.node
            ),
            vec![],
            vec![],
            "Declare EngineHostedProductRequired in NodeCatalog and publish outside NodeDefinition through WGSL @cce-node metadata, a shared Canonical Program registration, or an EngineHosted Hybrid Product registration.",
            "cce_product_authority",
        );
    };
    let Some(cce_planner) = &inputs.cce_planner_source else {
        return check(
            "cce_product_authority",
            "EngineHosted execution has one formal authority",
            CheckSeverity::Blocker,
            CheckStatus::Missing,
            "The canonical CCE planner source is unavailable, so the guard cannot prove that NodeProduct stages flow through the formal Loom optimization pass.".to_string(),
            vec![relative_path(&inputs.repo_dir, &node_definition.path)],
            vec![],
            "Restore crates/cunning_cce_plan/src/planner.rs as the only logical-plan to execution-plan authority.",
            "cce_product_authority",
        );
    };

    let publication_spans =
        formal_hosted_product_publication_hits(&inputs.repo_dir, node_definition, 12);
    let planner_spans = line_hits(
        cce_planner,
        &["optimize_cce_logical_plan(", "CceExecutionPlan"],
        8,
    );
    let host_safe_spans = line_hits(
        node_definition,
        &["NodeRuntimeBackendPolicy::RustHostSafe"],
        4,
    );
    let requires_hosted_product = node_definition
        .text
        .contains("NodeRuntimeBackendPolicy::EngineHostedProductRequired");
    let has_formal_product = !publication_spans.is_empty();
    let planner_is_formal = planner_spans
        .iter()
        .any(|span| span.line.contains("optimize_cce_logical_plan("))
        && planner_spans
            .iter()
            .any(|span| span.line.contains("CceExecutionPlan"));

    if requires_hosted_product && has_formal_product && planner_is_formal {
        let mut spans = publication_spans;
        spans.extend(planner_spans);
        return check(
            "cce_product_authority",
            "EngineHosted execution has one formal authority",
            CheckSeverity::Pass,
            CheckStatus::Pass,
            "The canonical node requires an EngineHosted product, a formal automatic/canonical/hybrid publication exists outside NodeDefinition, and plan_cce invokes the canonical Loom optimizer before producing CceExecutionPlan.".to_string(),
            vec![
                relative_path(&inputs.repo_dir, &node_definition.path),
                relative_path(&inputs.repo_dir, &cce_planner.path),
            ],
            spans,
            "Keep GPU algorithms in canonical WGSL, CPU algorithms in Rust stages, and scheduling in shared products; never add node-local IR, schedules, executors, or backend strategy.",
            "cce_product_authority",
        );
    }
    if !host_safe_spans.is_empty() && planner_is_formal {
        let mut spans = host_safe_spans;
        spans.extend(planner_spans);
        return check(
            "cce_product_authority",
            "EngineHosted execution has one formal authority",
            CheckSeverity::Pass,
            CheckStatus::Pass,
            "The canonical node explicitly permits a RustHostSafe stage. It may execute as Rust without entering Compute IR and must not create a WGPU device inside compute_engine.".to_string(),
            vec![
                relative_path(&inputs.repo_dir, &node_definition.path),
                relative_path(&inputs.repo_dir, &cce_planner.path),
            ],
            spans,
            "Keep Direct WGPU native. Add EngineHosted GPU coverage only through automatic WGSL publication, a shared Canonical Program, or a declarative Hybrid Product.",
            "cce_product_authority",
        );
    }

    let mut spans = publication_spans;
    spans.extend(planner_spans);
    check(
        "cce_product_authority",
        "EngineHosted execution has one formal authority",
        CheckSeverity::Blocker,
        CheckStatus::Fail,
        format!(
            "The canonical definition for '{}' either lacks EngineHostedProductRequired/RustHostSafe policy, has no formal automatic/canonical/hybrid publication outside NodeDefinition, or is not followed by the formal plan_cce Loom pass.",
            inputs.node
        ),
        vec![
            relative_path(&inputs.repo_dir, &node_definition.path),
            relative_path(&inputs.repo_dir, &cce_planner.path),
        ],
        spans,
        "Declare RustHostSafe only for a genuinely WGPU-free Rust stage. Nodes containing GPU work must declare EngineHostedProductRequired and publish through automatic WGSL metadata, a shared Canonical Program, or a declarative Hybrid Product.",
        "cce_product_authority",
    )
}

fn check_no_node_local_gpu_authority(inputs: &GuardInputs) -> ArchitectureCheck {
    let Some(node_definition) = &inputs.node_definition_source else {
        return check(
            "no_node_local_gpu_authority",
            "Node owns no parallel GPU execution authority",
            CheckSeverity::Blocker,
            CheckStatus::Missing,
            "The node definition is unavailable, so the guard cannot exclude node-local GPU authority.".to_string(),
            vec![],
            vec![],
            "Restore the canonical node definition and publish it through the shared CCE authoring APIs.",
            "cce_authority_blocker",
        );
    };
    let forbidden = [
        "NodeProductDescriptor::new",
        "NodeProductStageDescriptor::new",
        "NodeComputeProgramRef::new",
        "ComputeProgramEncoder::new",
        "ComputeProgramDescriptor {",
        "ComputeIrProgram",
        "ShaderIrModule",
        "ShaderProduct",
        "ShaderArtifact",
        "HeightFieldCookContract",
        "LoomRegionLowererDescriptor",
        "ReadyDagFieldPackage",
        "GeoCacheRef::Runtime",
        "HybridPeBoundary",
    ];
    let mut spans = line_hits(node_definition, &forbidden, 32);
    if let Some(node_source) = &inputs.node_source {
        spans.extend(line_hits(node_source, &forbidden, 32));
    }
    if let Some(substrate_source) = &inputs.substrate_source {
        spans.extend(line_hits(substrate_source, &forbidden, 32));
    }
    if spans.is_empty() {
        return check(
            "no_node_local_gpu_authority",
            "Node owns no parallel GPU execution authority",
            CheckSeverity::Pass,
            CheckStatus::Pass,
            "No node-local Shader IR, Compute IR, Hosted backend, or hand-built NodeProduct authority was found. Direct WGPU remains the native Cunning3D/Standalone path and is outside this prohibition.".to_string(),
            vec![relative_path(&inputs.repo_dir, &node_definition.path)],
            vec![],
            "Keep the node as a semantic publication shell over automatic ingestion or declarative shared products.",
            "cce_authority",
        );
    }
    check(
        "no_node_local_gpu_authority",
        "Node owns no parallel GPU execution authority",
        CheckSeverity::Blocker,
        CheckStatus::Fail,
        "The node contains forbidden Hosted product or IR authority. Direct WGPU is legal, but EngineHosted compilation and execution must remain centralized.".to_string(),
        vec![relative_path(&inputs.repo_dir, &node_definition.path)],
        spans,
        "Delete node-local product/IR/backend construction and route the canonical WGSL or Hybrid Recipe through the shared CCE authoring APIs.",
        "cce_authority_blocker",
    )
}

fn check_closed_world_node_gpu_authority(inputs: &GuardInputs) -> ArchitectureCheck {
    let mut spans = manual_node_product_authority_hits(&inputs.repo_dir, 32);
    spans.extend(competing_node_registry_authority_hits(
        &inputs.repo_dir,
        32usize.saturating_sub(spans.len()),
    ));
    spans.extend(untracked_explicit_parameter_projection_hits(
        &inputs.repo_dir,
        32usize.saturating_sub(spans.len()),
    ));
    if spans.is_empty() {
        return check(
            "closed_world_node_gpu_authority",
            "EngineHosted GPU authoring is a closed-world CCE contract",
            CheckSeverity::Pass,
            CheckStatus::Pass,
            "No node definition hand-builds Hosted product/IR authority, no competing static/dynamic NodeRegistry executor exists, and node-owned Hosted parameter expression or payload projection is zero. Direct WGPU schedules remain legal native execution and do not constitute CCE debt.".to_string(),
            vec![
                "crates/cunning_compute_products/src/parameter_recipe.rs".to_string(),
                "crates/cunning_compute_products/build.rs".to_string(),
            ],
            vec![],
            "Keep handwritten GPU math in canonical WGSL. NativeEditorWgpu and Standalone may schedule it directly; EngineHosted may only consume automatic ingestion, shared Canonical Programs, or declarative Hybrid Products.",
            "cce_authority",
        );
    }
    check(
        "closed_world_node_gpu_authority",
        "EngineHosted GPU authoring is a closed-world CCE contract",
        CheckSeverity::Blocker,
        CheckStatus::Fail,
        "A node definition hand-builds Hosted product/IR authority, a static/dynamic NodeRegistry executor bypass returned, or a node owns Hosted parameter-word projection. This lets future EngineHosted nodes bypass automatic ingestion or canonical program-owned parameter recipes.".to_string(),
        vec![
            "crates/cunning_compute_products/src/parameter_recipe.rs".to_string(),
        ],
        spans,
        "Delete the parallel authority. Put GPU math and validated packed-component semantics beside canonical WGSL, extend shared ingestion/program composition when needed, and reduce debt manifests only after removing production authority; never add or increase an entry.",
        "cce_authority_blocker",
    )
}

fn competing_node_registry_authority_hits(repo_dir: &Path, limit: usize) -> Vec<SourceSpan> {
    if limit == 0 {
        return Vec::new();
    }
    let path = repo_dir.join("src/cunning_core/registries/node_registry.rs");
    let Some(source) = read_source(path) else {
        return Vec::new();
    };
    line_hits(
        &source,
        &[
            "StaticNodeAuthority::Legacy",
            "pub fn register_dynamic_node",
            "self.insert_descriptor(desc)",
        ],
        limit,
    )
}

fn check_no_node_specific_runtime_projection_authority(inputs: &GuardInputs) -> ArchitectureCheck {
    let spans = node_specific_runtime_authority_hits(&inputs.repo_dir, &inputs.node, 32);
    if spans.is_empty() {
        return check(
            "no_node_specific_runtime_projection_authority",
            "Node owns no runtime parameter, lowering, or backend authority",
            CheckSeverity::Pass,
            CheckStatus::Pass,
            "No node-named runtime parameter packer, IR lowerer, recipe builder, binding table, executor, or backend path was found in the CCE production roots.".to_string(),
            vec![],
            vec![],
            "Keep parameter projection declarative and let shared CCE lowering evaluate it after reflected input layouts are known.",
            "cce_authority",
        );
    }
    check(
        "no_node_specific_runtime_projection_authority",
        "Node owns no runtime parameter, lowering, or backend authority",
        CheckSeverity::Blocker,
        CheckStatus::Fail,
        "A node-named runtime parameter packer, IR lowerer, recipe builder, binding table, executor, or backend path was found. Automatic publication cannot coexist with this second authority.".to_string(),
        vec![],
        spans,
        "Delete the node-specific runtime path. Express legal port/parameter semantics as declarative projections or shared canonical program topology, then let the generic CCE compiler and runtime lower them.",
        "cce_authority_blocker",
    )
}

fn check_no_runtime_parameter_packer_framework(inputs: &GuardInputs) -> ArchitectureCheck {
    let spans = runtime_parameter_packer_framework_hits(&inputs.repo_dir, 32);
    if spans.is_empty() {
        return check(
            "no_runtime_parameter_packer_framework",
            "CCE owns no callback-based parameter packer framework",
            CheckSeverity::Pass,
            CheckStatus::Pass,
            "The production CDA runtime contains no callback registry that can reintroduce node-specific parameter packing authority.".to_string(),
            vec![],
            vec![],
            "Keep uniform projection in reflected declarative data and genuine prepared arrays in typed PreparedUpload stages.",
            "cce_authority",
        );
    }
    check(
        "no_runtime_parameter_packer_framework",
        "CCE owns no callback-based parameter packer framework",
        CheckSeverity::Blocker,
        CheckStatus::Fail,
        "The production CDA runtime still exposes a callback-based parameter packer framework. This is a naming-independent escape hatch for future node forks, so every Gaea node promotion remains blocked until the framework and registrations are deleted.".to_string(),
        vec![],
        spans,
        "Move every remaining parameter payload to reflected declarative projections or typed PreparedUpload, then delete ComputeProgramParameterPacker and its inventory registry entirely.",
        "cce_authority_blocker",
    )
}

fn check_canonical_shader_authority(inputs: &GuardInputs) -> ArchitectureCheck {
    let forbidden_node_shader_source = [
        "@compute",
        "@group(",
        "RWStructuredBuffer",
        "[numthreads",
        ".compute\"",
        ".hlsl\"",
        "ShaderModuleDescriptor",
        "create_compute_pipeline",
        "create_shader_module",
    ];
    let forbidden_substrate_backend = ["ComputeShader.Dispatch", "FRDGBuilder"];
    let mut spans = inputs
        .node_definition_source
        .as_ref()
        .map(|source| line_hits(source, &forbidden_node_shader_source, 32))
        .unwrap_or_default();
    if let Some(source) = &inputs.node_source {
        spans.extend(line_hits(source, &forbidden_node_shader_source, 32));
        spans.extend(line_hits(source, &forbidden_substrate_backend, 32));
    }
    if let Some(source) = &inputs.substrate_source {
        spans.extend(line_hits(source, &forbidden_substrate_backend, 32));
    }
    if spans.is_empty() {
        return check(
            "canonical_shader_authority",
            "GPU algorithm source and backend ownership are canonical",
            CheckSeverity::Pass,
            CheckStatus::Pass,
            "No inline WGSL/HLSL, engine shader copy, or Unity/Unreal Hosted backend execution was found in the node publication or Rust substrate. Direct WGPU remains legal.".to_string(),
            vec![],
            vec![],
            "Keep handwritten GPU math in canonical WGSL. Direct WGPU consumes it natively; EngineHosted CCE generates reflection, programs, artifacts, and host execution.",
            "cce_authority",
        );
    }
    check(
        "canonical_shader_authority",
        "GPU algorithm source and backend ownership are canonical",
        CheckSeverity::Blocker,
        CheckStatus::Fail,
        "The node or Rust substrate contains inline shader text or Unity/Unreal backend authority outside the shared EngineHosted CCE path.".to_string(),
        vec![],
        spans,
        "Move GPU math to canonical WGSL and delete inline shader text, engine shader copies, and Unity/Unreal backend execution. Do not delete the native Direct WGPU path.",
        "cce_authority_blocker",
    )
}

fn check_performance_claims(inputs: &GuardInputs) -> ArchitectureCheck {
    let Some(matrix) = &inputs.acceptance_matrix else {
        return check(
            "performance_claim",
            "Performance claims are backed by structured rows",
            CheckSeverity::Warning,
            CheckStatus::Warn,
            "The performance acceptance matrix is missing or unreadable.".to_string(),
            vec![],
            vec![],
            "Restore the acceptance matrix before making speedup or GPU-first claims.",
            "performance_claim",
        );
    };
    let rows = matrix
        .get("rows")
        .and_then(Value::as_array)
        .map(|rows| {
            rows.iter()
                .filter(|row| {
                    row.get("node")
                        .and_then(Value::as_str)
                        .map(|candidate| candidate.eq_ignore_ascii_case(&inputs.node))
                        .unwrap_or(false)
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if rows.is_empty() {
        return check(
            "performance_claim",
            "Performance claims are backed by structured rows",
            CheckSeverity::Warning,
            CheckStatus::Warn,
            format!(
                "No acceptance-matrix row exists for '{}'. Do not make speed or GPU promotion claims from ad-hoc timing.",
                inputs.node
            ),
            vec![],
            vec![],
            "Add an acceptance row with raw gate artifact, baseline source, readback/materialization evidence, and promotion status before claiming performance.",
            "performance_claim",
        );
    }
    let missing_evidence = rows
        .iter()
        .filter(|row| {
            row.get("raw_gate_artifact")
                .and_then(Value::as_str)
                .is_none()
                || row.get("baseline_source").and_then(Value::as_str).is_none()
                || row
                    .get("promotion_status")
                    .and_then(Value::as_str)
                    .is_none()
        })
        .count();
    if missing_evidence > 0 {
        return check(
            "performance_claim",
            "Performance claims are backed by structured rows",
            CheckSeverity::Blocker,
            CheckStatus::Fail,
            format!(
                "{missing_evidence} acceptance row(s) for '{}' are missing raw gate artifact, baseline source, or promotion status.",
                inputs.node
            ),
            vec![relative_path(
                &inputs.repo_dir,
                &inputs
                    .repo_dir
                    .join("tools")
                    .join("c3d_devflywheeltool")
                    .join(NODE_PERFORMANCE_ACCEPTANCE_MATRIX_PATH),
            )],
            vec![],
            "Fill the matrix with raw hashes/artifacts, baseline source, readback/materialization counters, cache evidence, and promotion status.",
            "performance_claim",
        );
    }
    check(
        "performance_claim",
        "Performance claims are backed by structured rows",
        CheckSeverity::Pass,
        CheckStatus::Pass,
        format!(
            "{} acceptance row(s) for '{}' contain the required raw gate, baseline, and promotion fields.",
            rows.len(),
            inputs.node
        ),
        vec![relative_path(
            &inputs.repo_dir,
            &inputs
                .repo_dir
                .join("tools")
                .join("c3d_devflywheeltool")
                .join(NODE_PERFORMANCE_ACCEPTANCE_MATRIX_PATH),
        )],
        vec![],
        "Keep speedup claims separated from correctness, barrier truth, executed regions, readbacks, and cache hit/miss evidence.",
        "performance_claim",
    )
}

impl GuardInputs {
    fn load(ctx: &Context, node: &str) -> Self {
        let repo_dir = ctx.root.clone();
        let node_key = normalize_key(node);
        let node_snake = snake_case(node);
        let node_source =
            read_first_existing(&node_source_candidates(&repo_dir, node, &node_snake));
        let substrate_source =
            read_first_existing(&substrate_source_candidates(&repo_dir, node, &node_snake));
        let node_type_symbols = node_source
            .as_ref()
            .map(extract_node_type_symbols)
            .filter(|symbols| !symbols.is_empty())
            .unwrap_or_else(|| fallback_node_type_symbols(node, &node_snake));
        let node_definition_source =
            find_node_definition_source(&repo_dir, node, &node_type_symbols);
        let cce_planner_source = read_source(
            repo_dir
                .join("crates")
                .join("cunning_cce_plan")
                .join("src")
                .join("planner.rs"),
        );
        let decompiled_source = find_decompiled_source(ctx, node, &node_key);
        let acceptance_matrix = read_json_value(
            repo_dir
                .join("tools")
                .join("c3d_devflywheeltool")
                .join(NODE_PERFORMANCE_ACCEPTANCE_MATRIX_PATH),
        );
        Self {
            repo_dir,
            node: node.to_string(),
            node_source,
            substrate_source,
            node_definition_source,
            cce_planner_source,
            decompiled_source,
            acceptance_matrix,
        }
    }
}

fn check(
    id: &'static str,
    title: &'static str,
    severity: CheckSeverity,
    status: CheckStatus,
    message: String,
    evidence_refs: Vec<String>,
    source_spans: Vec<SourceSpan>,
    suggested_fix: &str,
    metra_card_kind: &'static str,
) -> ArchitectureCheck {
    ArchitectureCheck {
        id,
        title,
        severity,
        status,
        message,
        evidence_refs,
        source_spans,
        suggested_fix: suggested_fix.to_string(),
        metra_card_kind,
    }
}

fn gui_sections() -> Vec<ArchitectureGuiSection> {
    vec![
        ArchitectureGuiSection {
            id: "code_shape",
            title: "Code Shape",
            card_kind: "architecture_guard_group",
            check_ids: vec![
                "node_wrapper_shape",
                "substrate_placement",
                "materialization_path",
            ],
        },
        ArchitectureGuiSection {
            id: "contracts",
            title: "Contracts",
            card_kind: "architecture_guard_group",
            check_ids: vec![
                "surface_contract",
                "node_catalog_loom_publication",
                "cce_product_authority",
                "no_node_local_gpu_authority",
                "closed_world_node_gpu_authority",
                "no_node_specific_runtime_projection_authority",
                "no_runtime_parameter_packer_framework",
                "canonical_shader_authority",
            ],
        },
        ArchitectureGuiSection {
            id: "runtime",
            title: "Runtime",
            card_kind: "architecture_guard_group",
            check_ids: vec!["residency_path", "performance_claim"],
        },
    ]
}

fn next_commands(node: &str) -> Vec<String> {
    vec![
        format!("{TOOL_COMMAND} reverse --node {node} --json"),
        format!("{TOOL_COMMAND} status --node {node} --json"),
        format!("{TOOL_COMMAND} architecture-guard --node {node} --json --strict"),
        format!("{TOOL_COMMAND} verify --node {node} --json --strict"),
    ]
}

fn node_source_candidates(repo_dir: &Path, node: &str, node_snake: &str) -> Vec<PathBuf> {
    let lower = node.to_ascii_lowercase();
    vec![
        repo_dir
            .join("src")
            .join("nodes")
            .join("heightfield")
            .join(format!("{node_snake}.rs")),
        repo_dir
            .join("src")
            .join("nodes")
            .join("heightfield")
            .join(format!("{lower}.rs")),
    ]
}

fn substrate_source_candidates(repo_dir: &Path, node: &str, node_snake: &str) -> Vec<PathBuf> {
    let lower = node.to_ascii_lowercase();
    vec![
        repo_dir
            .join("src")
            .join("cunning_core")
            .join("core")
            .join("geometry")
            .join("heightfield")
            .join(format!("{node_snake}.rs")),
        repo_dir
            .join("src")
            .join("cunning_core")
            .join("core")
            .join("geometry")
            .join("heightfield")
            .join(format!("{lower}.rs")),
    ]
}

fn find_node_definition_source(
    repo_dir: &Path,
    node: &str,
    node_type_symbols: &[String],
) -> Option<NodeSource> {
    let directory = repo_dir
        .join("crates")
        .join("cunning_core")
        .join("src")
        .join("node_definitions");
    let node_key = normalize_key(node);
    let mut paths = fs::read_dir(directory)
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|extension| extension.to_str()) == Some("rs"))
        .filter(|path| path.file_stem().and_then(|stem| stem.to_str()) != Some("mod"))
        .collect::<Vec<_>>();
    paths.sort();
    paths.into_iter().find_map(|path| {
        let source = read_source(path)?;
        let stem_matches = source
            .path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .map(normalize_key)
            .is_some_and(|stem| stem == node_key);
        let symbol_matches = node_type_symbols.iter().any(|symbol| {
            source
                .lines
                .iter()
                .any(|line| line_contains_symbol(line, symbol))
        });
        let identity_matches = source.lines.iter().any(|line| {
            (line.contains("LEGACY_LOAD_NAME") || line.contains("EDITOR_NAME"))
                && quoted_value(line).is_some_and(|value| normalize_key(value) == node_key)
        });
        (stem_matches || symbol_matches || identity_matches).then_some(source)
    })
}

fn quoted_value(line: &str) -> Option<&str> {
    let start = line.find('"')? + 1;
    let end = line[start..].find('"')? + start;
    Some(&line[start..end])
}

fn read_first_existing(paths: &[PathBuf]) -> Option<NodeSource> {
    paths.iter().find_map(|path| read_source(path.clone()))
}

fn read_source(path: PathBuf) -> Option<NodeSource> {
    let text = fs::read_to_string(&path).ok()?;
    let lines = text.lines().map(str::to_string).collect::<Vec<_>>();
    Some(NodeSource { path, text, lines })
}

fn read_json_value(path: PathBuf) -> Option<Value> {
    let text = fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

fn formal_hosted_product_publication_hits(
    repo_dir: &Path,
    node_definition: &NodeSource,
    limit: usize,
) -> Vec<SourceSpan> {
    let Some(type_id) = node_definition.lines.iter().find_map(|line| {
        (line.contains("TYPE_ID") && line.contains('='))
            .then(|| quoted_value(line))
            .flatten()
    }) else {
        return Vec::new();
    };
    let mut paths = Vec::new();
    for relative in [
        "crates/cunning_compute_products/src",
        "crates/cunning_engine_hosted_cce/src",
    ] {
        collect_sources_with_extensions(&repo_dir.join(relative), &["rs"], &mut paths);
    }
    collect_sources_with_extensions(&repo_dir.join("src"), &["wgsl"], &mut paths);
    paths.sort();

    let mut spans = Vec::new();
    for path in paths {
        let Some(source) = read_source(path) else {
            continue;
        };
        let is_wgsl = source.path.extension().and_then(|value| value.to_str()) == Some("wgsl");
        let formal_rust = source.text.contains(type_id)
            && (source.text.contains("EngineHostedNodeProgramRegistration")
                || source
                    .text
                    .contains("EngineHostedHeightfieldProductRegistration"));
        for (index, line) in source.lines.iter().enumerate() {
            let formal_wgsl = is_wgsl && line.contains("@cce-node|") && line.contains(type_id);
            let formal_rust_line = formal_rust
                && (line.contains(type_id)
                    || line.contains("EngineHostedNodeProgramRegistration")
                    || line.contains("EngineHostedHeightfieldProductRegistration"));
            if formal_wgsl || formal_rust_line {
                spans.push(SourceSpan {
                    path: relative_path(repo_dir, &source.path),
                    line_number: index + 1,
                    line: line.trim().to_string(),
                });
                if spans.len() >= limit {
                    return spans;
                }
            }
        }
    }
    spans
}

fn node_specific_runtime_authority_hits(
    repo_dir: &Path,
    node: &str,
    limit: usize,
) -> Vec<SourceSpan> {
    let node_snake = snake_case(node);
    if node_snake.is_empty() {
        return Vec::new();
    }
    let exact_needles = [
        format!("{node_snake}_parameter_packer"),
        format!("pack_{node_snake}_parameters"),
        format!("lower_{node_snake}_to_compute_ir"),
        format!("lower_{node_snake}_compute"),
        format!("{node_snake}_compute_ir_builder"),
        format!("{node_snake}_shader_ir_builder"),
        format!("{node_snake}_recipe_builder"),
        format!("{node_snake}_runtime_executor"),
        format!("{node_snake}_binding_table"),
        format!("{node_snake}_backend"),
    ]
    .map(|needle| normalize_key(needle.as_str()));
    let roots = [
        "crates/cunning_cda_runtime/src",
        "crates/cunning_cce_plan/src",
        "crates/cunning_compute_products/src",
        "crates/cunning_compute_core/src",
        "crates/cunning_compute_ir/src",
        "crates/cunning_shader_ir/src",
        "crates/cunning_engine_hosted_cce/src",
        "crates/cunning_engine_hosted_runtime/src",
    ];
    let mut paths = Vec::new();
    for relative in roots {
        collect_rust_sources(&repo_dir.join(relative), &mut paths);
    }
    paths.sort();

    let mut spans = Vec::new();
    for path in paths {
        let Some(source) = read_source(path) else {
            continue;
        };
        for (index, line) in source.lines.iter().enumerate() {
            let normalized_line = normalize_key(line);
            if exact_needles
                .iter()
                .any(|needle| normalized_line.contains(needle.as_str()))
            {
                spans.push(SourceSpan {
                    path: source.path.display().to_string(),
                    line_number: index + 1,
                    line: line.trim().to_string(),
                });
                if spans.len() >= limit {
                    return spans;
                }
            }
        }
    }
    spans
}

fn runtime_parameter_packer_framework_hits(repo_dir: &Path, limit: usize) -> Vec<SourceSpan> {
    let root = repo_dir.join("crates/cunning_cda_runtime/src");
    let mut paths = Vec::new();
    collect_rust_sources(&root, &mut paths);
    paths.sort();

    let mut spans = Vec::new();
    for path in paths {
        let Some(source) = read_source(path) else {
            continue;
        };
        for (index, line) in source.lines.iter().enumerate() {
            let normalized = line.to_ascii_lowercase();
            let callback_type = line.contains("ComputeProgramParameterPacker");
            let node_named_factory = normalized.contains("_parameter_packer");
            let node_named_pack = normalized.contains("pack_")
                && normalized.contains("_parameters")
                && !normalized.contains("pack_projected_")
                && !normalized.contains("pack_automatic_");
            if callback_type || node_named_factory || node_named_pack {
                spans.push(SourceSpan {
                    path: source.path.display().to_string(),
                    line_number: index + 1,
                    line: line.trim().to_string(),
                });
                if spans.len() >= limit {
                    return spans;
                }
            }
        }
    }
    spans
}

fn manual_node_product_authority_hits(repo_dir: &Path, limit: usize) -> Vec<SourceSpan> {
    let root = repo_dir.join("crates/cunning_core/src/node_definitions");
    let mut paths = Vec::new();
    collect_rust_sources(&root, &mut paths);
    paths.sort();
    let forbidden = [
        "NodeProductDescriptor::new",
        "NodeProductStageDescriptor::new",
        "NodeComputeProgramRef::new",
        "ComputeProgramEncoder::new",
        "ComputeProgramDescriptor {",
        "ComputeIrProgram",
        "ShaderIrModule",
        "create_compute_pipeline(",
        "begin_compute_pass(",
        "dispatch_workgroups(",
        "queue.submit(",
    ];
    let mut spans = Vec::new();
    for path in paths {
        let Some(source) = read_source(path) else {
            continue;
        };
        spans.extend(line_hits(
            &source,
            &forbidden,
            limit.saturating_sub(spans.len()),
        ));
        if spans.len() >= limit {
            break;
        }
    }
    spans
}

fn untracked_explicit_parameter_projection_hits(repo_dir: &Path, limit: usize) -> Vec<SourceSpan> {
    if limit == 0 {
        return Vec::new();
    }
    let mut paths = Vec::new();
    collect_rust_sources(
        &repo_dir.join("crates/cunning_core/src/node_definitions"),
        &mut paths,
    );
    paths.sort();
    let mut spans = Vec::new();
    for path in paths {
        let Some(source) = read_source(path) else {
            continue;
        };
        if !source.text.contains("NodeComputeParameterWordExpr")
            && !source.text.contains("NodeComputeParameterBlockProjection")
        {
            continue;
        }
        spans.extend(line_hits(
            &source,
            &[
                "NodeComputeParameterWordExpr",
                "NodeComputeParameterBlockProjection",
            ],
            limit.saturating_sub(spans.len()),
        ));
        if spans.len() >= limit {
            break;
        }
    }
    spans
}

fn collect_rust_sources(root: &Path, paths: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_rust_sources(&path, paths);
        } else if path.extension().and_then(|extension| extension.to_str()) == Some("rs") {
            paths.push(path);
        }
    }
}

fn collect_sources_with_extensions(root: &Path, extensions: &[&str], paths: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_sources_with_extensions(&path, extensions, paths);
        } else if path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extensions.contains(&extension))
        {
            paths.push(path);
        }
    }
}

fn find_decompiled_source(ctx: &Context, node: &str, node_key: &str) -> Option<NodeSource> {
    let roots = vec![
        ctx.gaea_decompiled_root.join("Gaea.Nodes"),
        ctx.gaea_decompiled_root.join("Gaea"),
    ];
    let mut stack = roots
        .iter()
        .cloned()
        .filter(|root| root.exists())
        .collect::<Vec<_>>();
    while let Some(dir) = stack.pop() {
        let entries = fs::read_dir(&dir).ok()?;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if path.extension().and_then(|ext| ext.to_str()) != Some("cs") {
                continue;
            }
            let file_key = path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .map(normalize_key)
                .unwrap_or_default();
            if file_key == node_key || file_key.contains(node_key) {
                if let Some(source) = read_source(path) {
                    return Some(source);
                }
            }
        }
    }
    let fallback_key = normalize_key(node);
    roots
        .iter()
        .filter(|root| root.exists())
        .find_map(|root| find_source_by_text(root, &fallback_key))
}

fn find_source_by_text(root: &Path, node_key: &str) -> Option<NodeSource> {
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir).ok()?.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if path.extension().and_then(|ext| ext.to_str()) != Some("cs") {
                continue;
            }
            let source = read_source(path)?;
            if normalize_key(&source.text).contains(node_key) {
                return Some(source);
            }
        }
    }
    None
}

fn extract_node_type_symbols(source: &NodeSource) -> Vec<String> {
    let mut symbols = Vec::new();
    for line in &source.lines {
        let mut offset = 0usize;
        while let Some(index) = line[offset..].find("NODE_HEIGHTFIELD") {
            let start = offset + index;
            let tail = &line[start..];
            let end = tail
                .find(|ch: char| !(ch.is_ascii_uppercase() || ch.is_ascii_digit() || ch == '_'))
                .unwrap_or(tail.len());
            let symbol = tail[..end].to_string();
            if !symbols.contains(&symbol) {
                symbols.push(symbol);
            }
            offset = start + end;
        }
    }
    symbols
}

fn fallback_node_type_symbols(node: &str, node_snake: &str) -> Vec<String> {
    let upper = node_snake.to_ascii_uppercase();
    vec![
        format!("NODE_HEIGHTFIELD_{upper}"),
        format!("NODE_HEIGHTFIELD_{}", node.to_ascii_uppercase()),
    ]
}

fn line_hits(source: &NodeSource, needles: &[&str], limit: usize) -> Vec<SourceSpan> {
    let mut spans = Vec::new();
    for (index, line) in source.lines.iter().enumerate() {
        if needles.iter().any(|needle| line.contains(needle)) {
            spans.push(SourceSpan {
                path: source.path.display().to_string(),
                line_number: index + 1,
                line: line.trim().to_string(),
            });
            if spans.len() >= limit {
                break;
            }
        }
    }
    spans
}

fn symbol_hits(source: &NodeSource, symbols: &[String], limit: usize) -> Vec<SourceSpan> {
    let mut spans = Vec::new();
    for (index, line) in source.lines.iter().enumerate() {
        if symbols
            .iter()
            .any(|symbol| line_contains_symbol(line, symbol))
        {
            spans.push(SourceSpan {
                path: source.path.display().to_string(),
                line_number: index + 1,
                line: line.trim().to_string(),
            });
            if spans.len() >= limit {
                break;
            }
        }
    }
    spans
}

fn line_contains_symbol(line: &str, symbol: &str) -> bool {
    let mut search_from = 0usize;
    while let Some(index) = line[search_from..].find(symbol) {
        let start = search_from + index;
        let end = start + symbol.len();
        let before_ok = line[..start]
            .chars()
            .next_back()
            .map(|ch| !(ch.is_ascii_alphanumeric() || ch == '_'))
            .unwrap_or(true);
        let after_ok = line[end..]
            .chars()
            .next()
            .map(|ch| !(ch.is_ascii_alphanumeric() || ch == '_'))
            .unwrap_or(true);
        if before_ok && after_ok {
            return true;
        }
        search_from = end;
    }
    false
}

fn relative_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .display()
        .to_string()
}

fn snake_case(value: &str) -> String {
    let mut out = String::new();
    let mut previous_is_lower_or_digit = false;
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() {
            if ch.is_ascii_uppercase() && previous_is_lower_or_digit && !out.ends_with('_') {
                out.push('_');
            }
            out.push(ch.to_ascii_lowercase());
            previous_is_lower_or_digit = ch.is_ascii_lowercase() || ch.is_ascii_digit();
        } else {
            if !out.ends_with('_') && !out.is_empty() {
                out.push('_');
            }
            previous_is_lower_or_digit = false;
        }
    }
    out.trim_matches('_').to_string()
}

fn normalize_key(value: &str) -> String {
    value
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source(path: &str, text: &str) -> NodeSource {
        NodeSource {
            path: PathBuf::from(path),
            text: text.to_string(),
            lines: text.lines().map(str::to_string).collect(),
        }
    }

    fn inputs(definition: &str, planner: &str) -> GuardInputs {
        GuardInputs {
            repo_dir: PathBuf::from("repo"),
            node: "Fixture".to_string(),
            node_source: None,
            substrate_source: None,
            node_definition_source: Some(source(
                "repo/crates/cunning_core/src/node_definitions/fixture.rs",
                definition,
            )),
            cce_planner_source: Some(source(
                "repo/crates/cunning_cce_plan/src/planner.rs",
                planner,
            )),
            decompiled_source: None,
            acceptance_matrix: None,
        }
    }

    #[test]
    fn catalog_gate_requires_explicit_canonical_loom_publication() {
        let accepted = inputs(
            "NodeCatalogEntry::new();\nNodePublication {\n    loom: true,\n}",
            "",
        );
        let rejected = inputs(
            "NodeCatalogEntry::new();\nNodePublication {\n    loom: false,\n}",
            "",
        );

        assert_eq!(
            check_node_catalog_loom_publication(&accepted).status,
            CheckStatus::Pass
        );
        assert_eq!(
            check_node_catalog_loom_publication(&rejected).status,
            CheckStatus::Fail
        );
    }

    #[test]
    fn cce_gate_accepts_automatic_wgsl_ingestion_and_formal_planner() {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let repo_dir = std::env::temp_dir().join(format!(
            "c3d_architecture_guard_automatic_publication_{}_{}",
            std::process::id(),
            nonce
        ));
        let shader_dir = repo_dir.join("src/cunning_core/core/geometry/heightfield");
        fs::create_dir_all(&shader_dir).unwrap();
        fs::write(
            shader_dir.join("fixture.wgsl"),
            "// @cce-node|fixture_main|cunning.heightfield_fixture\n",
        )
        .unwrap();
        let mut inputs = inputs(
            "pub const TYPE_ID: &str = \"cunning.heightfield_fixture\";\nNodeRuntimeBackendPolicy::EngineHostedProductRequired",
            "fn plan_cce() -> CceExecutionPlan {\noptimize_cce_logical_plan(\nunimplemented!()\n}",
        );
        inputs.repo_dir = repo_dir.clone();

        assert_eq!(
            check_cce_product_authority(&inputs).status,
            CheckStatus::Pass
        );
        fs::remove_dir_all(repo_dir).unwrap();
    }

    #[test]
    fn cce_gate_accepts_declarative_hybrid_composition() {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let repo_dir = std::env::temp_dir().join(format!(
            "c3d_architecture_guard_hybrid_publication_{}_{}",
            std::process::id(),
            nonce
        ));
        let product_dir = repo_dir.join("crates/cunning_engine_hosted_cce/src");
        fs::create_dir_all(&product_dir).unwrap();
        fs::write(
            product_dir.join("fixture_product.rs"),
            "const TYPE_ID: &str = \"cunning.heightfield_fixture\";\nEngineHostedHeightfieldProductRegistration { describe: fixture_product };\n",
        )
        .unwrap();
        let mut inputs = inputs(
            "pub const TYPE_ID: &str = \"cunning.heightfield_fixture\";\nNodeRuntimeBackendPolicy::EngineHostedProductRequired",
            "fn plan_cce() -> CceExecutionPlan {\noptimize_cce_logical_plan(\nunimplemented!()\n}",
        );
        inputs.repo_dir = repo_dir.clone();

        assert_eq!(
            check_cce_product_authority(&inputs).status,
            CheckStatus::Pass
        );
        fs::remove_dir_all(repo_dir).unwrap();
    }

    #[test]
    fn authority_gate_rejects_hand_built_node_product() {
        let inputs = inputs(
            "NodeProductDescriptor::new();\nNodeComputeProgramRef::new();",
            "fn plan_cce() -> CceExecutionPlan {\noptimize_cce_logical_plan(\nunimplemented!()\n}",
        );

        assert_eq!(
            check_no_node_local_gpu_authority(&inputs).status,
            CheckStatus::Fail
        );
    }

    #[test]
    fn authority_gate_rejects_retired_loom_node_authority() {
        let inputs = inputs(
            "HeightFieldCookContract::new();\nLoomRegionLowererDescriptor::new();\nReadyDagFieldPackage::new();",
            "fn plan_cce() -> CceExecutionPlan {\noptimize_cce_logical_plan(\nunimplemented!()\n}",
        );

        assert_eq!(
            check_no_node_local_gpu_authority(&inputs).status,
            CheckStatus::Fail
        );
    }

    #[test]
    fn authority_gate_rejects_node_local_shader_and_program_construction() {
        let inputs = inputs(
            "let shader = \"@compute @workgroup_size(8, 8, 1) fn main() {}\";\nComputeProgramEncoder::new();",
            "fn plan_cce() -> CceExecutionPlan {\noptimize_cce_logical_plan(\nunimplemented!()\n}",
        );

        assert_eq!(
            check_no_node_local_gpu_authority(&inputs).status,
            CheckStatus::Fail
        );
        assert_eq!(
            check_canonical_shader_authority(&inputs).status,
            CheckStatus::Fail
        );
    }

    #[test]
    fn authority_gate_rejects_node_named_runtime_parameter_packer() {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let repo_dir = std::env::temp_dir().join(format!(
            "c3d_architecture_guard_parameter_packer_{}_{}",
            std::process::id(),
            nonce
        ));
        let runtime_dir = repo_dir.join("crates/cunning_cda_runtime/src/compute_lowerer");
        fs::create_dir_all(&runtime_dir).unwrap();
        fs::write(
            runtime_dir.join("product_implementations.rs"),
            "fn FixtureParameterPacker() {}\nfn pack_fixture_parameters() {}\n",
        )
        .unwrap();
        let mut inputs = inputs(
            "automatic_compute_implementation(\n    &definition,\n    registration,\n)",
            "fn plan_cce() -> CceExecutionPlan {\noptimize_cce_logical_plan(\nunimplemented!()\n}",
        );
        inputs.repo_dir = repo_dir.clone();

        assert_eq!(
            check_no_node_specific_runtime_projection_authority(&inputs).status,
            CheckStatus::Fail
        );
        fs::remove_dir_all(repo_dir).unwrap();
    }

    #[test]
    fn authority_gate_rejects_parameter_packer_framework_independent_of_node_name() {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let repo_dir = std::env::temp_dir().join(format!(
            "c3d_architecture_guard_global_parameter_packer_{}_{}",
            std::process::id(),
            nonce
        ));
        let runtime_dir = repo_dir.join("crates/cunning_cda_runtime/src");
        fs::create_dir_all(&runtime_dir).unwrap();
        fs::write(
            runtime_dir.join("projection_escape_hatch.rs"),
            "struct ComputeProgramParameterPacker;\nfn unrelated_parameter_packer() {}\n",
        )
        .unwrap();
        let mut inputs = inputs(
            "automatic_compute_implementation(\n    &definition,\n    registration,\n)",
            "fn plan_cce() -> CceExecutionPlan {\noptimize_cce_logical_plan(\nunimplemented!()\n}",
        );
        inputs.repo_dir = repo_dir.clone();

        assert_eq!(
            check_no_runtime_parameter_packer_framework(&inputs).status,
            CheckStatus::Fail
        );
        fs::remove_dir_all(repo_dir).unwrap();
    }

    #[test]
    fn authority_gate_rejects_product_authority_hidden_in_node_substrate() {
        let mut inputs = inputs(
            "automatic_compute_implementation(\n    &definition,\n    registration,\n)",
            "fn plan_cce() -> CceExecutionPlan {\noptimize_cce_logical_plan(\nunimplemented!()\n}",
        );
        inputs.substrate_source = Some(source(
            "repo/src/cunning_core/core/geometry/heightfield/fixture.rs",
            "ComputeProgramEncoder::new();\nShaderIrModule::default();",
        ));

        assert_eq!(
            check_no_node_local_gpu_authority(&inputs).status,
            CheckStatus::Fail
        );
    }

    #[test]
    fn closed_world_gate_rejects_manual_product_in_any_node_definition() {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let repo_dir = std::env::temp_dir().join(format!(
            "c3d_architecture_guard_manual_product_{}_{}",
            std::process::id(),
            nonce
        ));
        let definitions = repo_dir.join("crates/cunning_core/src/node_definitions");
        fs::create_dir_all(&definitions).unwrap();
        fs::write(
            definitions.join("unrelated.rs"),
            "fn publish() { NodeProductDescriptor::new(); }\n",
        )
        .unwrap();
        let mut inputs = inputs(
            "automatic_compute_implementation(&definition, registration)",
            "fn plan_cce() -> CceExecutionPlan { optimize_cce_logical_plan(); unimplemented!() }",
        );
        inputs.repo_dir = repo_dir.clone();

        assert_eq!(
            check_closed_world_node_gpu_authority(&inputs).status,
            CheckStatus::Fail
        );
        fs::remove_dir_all(repo_dir).unwrap();
    }

    #[test]
    fn closed_world_gate_allows_native_direct_wgpu_schedule() {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let repo_dir = std::env::temp_dir().join(format!(
            "c3d_architecture_guard_raw_wgpu_growth_{}_{}",
            std::process::id(),
            nonce
        ));
        let nodes = repo_dir.join("src/nodes/heightfield");
        fs::create_dir_all(&nodes).unwrap();
        fs::write(
            nodes.join("future_node.rs"),
            "fn run(device: &wgpu::Device) { let _pipeline = device.create_compute_pipeline(todo!()); }\n",
        )
        .unwrap();
        let mut inputs = inputs(
            "automatic_compute_implementation(&definition, registration)",
            "fn plan_cce() -> CceExecutionPlan { optimize_cce_logical_plan(); unimplemented!() }",
        );
        inputs.repo_dir = repo_dir.clone();

        assert_eq!(
            check_closed_world_node_gpu_authority(&inputs).status,
            CheckStatus::Pass
        );
        fs::remove_dir_all(repo_dir).unwrap();
    }

    #[test]
    fn authority_gate_accepts_shared_program_reference_without_inline_shader() {
        let inputs = inputs(
            "automatic_compute_implementation(\n    &definition,\n    product::compute_program_descriptor(),\n)",
            "fn plan_cce() -> CceExecutionPlan {\noptimize_cce_logical_plan(\nunimplemented!()\n}",
        );

        assert_eq!(
            check_canonical_shader_authority(&inputs).status,
            CheckStatus::Pass
        );
    }

    #[test]
    fn cce_gate_rejects_untyped_node_local_execution() {
        let inputs = inputs(
            "NodeCatalogEntry::new();\nfn execute_node_locally() {}",
            "fn plan_cce() -> CceExecutionPlan {\noptimize_cce_logical_plan(\nunimplemented!()\n}",
        );

        assert_eq!(
            check_cce_product_authority(&inputs).status,
            CheckStatus::Fail
        );
    }
}
