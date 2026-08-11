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
