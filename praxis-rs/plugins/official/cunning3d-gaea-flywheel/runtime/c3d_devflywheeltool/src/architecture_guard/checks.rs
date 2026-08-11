use super::*;

pub(super) fn check_node_wrapper_shape(inputs: &GuardInputs) -> ArchitectureCheck {
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

pub(super) fn check_substrate_placement(inputs: &GuardInputs) -> ArchitectureCheck {
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

pub(super) fn check_surface_contract(inputs: &GuardInputs) -> ArchitectureCheck {
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

pub(super) fn check_node_catalog_loom_publication(inputs: &GuardInputs) -> ArchitectureCheck {
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

pub(super) fn check_residency_path(inputs: &GuardInputs) -> ArchitectureCheck {
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

pub(super) fn check_materialization_path(inputs: &GuardInputs) -> ArchitectureCheck {
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

pub(super) fn check_cce_product_authority(inputs: &GuardInputs) -> ArchitectureCheck {
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

pub(super) fn check_no_node_local_gpu_authority(inputs: &GuardInputs) -> ArchitectureCheck {
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

pub(super) fn check_closed_world_node_gpu_authority(inputs: &GuardInputs) -> ArchitectureCheck {
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

pub(super) fn competing_node_registry_authority_hits(
    repo_dir: &Path,
    limit: usize,
) -> Vec<SourceSpan> {
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

pub(super) fn check_no_node_specific_runtime_projection_authority(
    inputs: &GuardInputs,
) -> ArchitectureCheck {
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

pub(super) fn check_no_runtime_parameter_packer_framework(
    inputs: &GuardInputs,
) -> ArchitectureCheck {
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

pub(super) fn check_canonical_shader_authority(inputs: &GuardInputs) -> ArchitectureCheck {
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

pub(super) fn check_performance_claims(inputs: &GuardInputs) -> ArchitectureCheck {
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
