use super::*;

#[test]
fn validates_the_minimal_focused_capture_contract() {
    let case = json!({
        "case_id": "node/case",
        "node": "expand",
        "input": { "domains": {}, "discrete_buffer_serialization": "{}" },
        "output": { "domains": {}, "discrete_buffer_serialization": "{}" },
        "deterministic_repeat_exact": true,
    });
    let capture = json!({
        "schema": HOUDINI_GROUP_CAPTURE_SCHEMA,
        "provider": { "id": "houdini" },
        "subject": { "id": "group" },
        "matrix": "focused",
        "cases": vec![case; 8],
    });
    validate_houdini_group_capture(&capture, "focused").unwrap();
}

#[test]
fn validates_native_case_counts() {
    let capture = json!({
        "schema": HOUDINI_GROUP_CAPTURE_SCHEMA,
        "provider": { "id": "cunning3d" },
        "subject": { "id": "group" },
        "matrix": "focused",
        "cases": vec![json!({
            "deterministic_repeat_exact": true,
            "input": { "discrete_buffer_serialization": "{}" },
            "output": { "discrete_buffer_serialization": "{}" },
        }); 8],
    });
    validate_native_group_capture(&capture, "focused").unwrap();
}

#[test]
fn semantic_capture_requires_every_unique_matrix_case() {
    let cases = group_semantic_case_ids()
        .into_iter()
        .map(|case_id| json!({ "case_id": case_id }))
        .collect::<Vec<_>>();
    validate_group_semantic_case_ids(&cases).unwrap();
    assert!(validate_group_semantic_case_ids(&cases[1..]).is_err());
    let mut duplicated = cases.clone();
    duplicated[1] = duplicated[0].clone();
    assert!(validate_group_semantic_case_ids(&duplicated).is_err());
}

#[test]
fn comparator_is_exact_for_sequences_and_tolerant_only_for_floats() {
    let expected = json!({ "cases": [{
        "case_id": "path/ordered",
        "parameters": { "mode": 0 },
        "input": { "domains": {} },
        "output": { "domains": { "point": {
            "positions": [[1.0, 2.0, 3.0]],
            "groups": { "out": { "members": [1, 2], "ordered_members": [2, 1] } }
        } } }
    }] });
    let mut actual = expected.clone();
    actual["cases"][0]["output"]["domains"]["point"]["positions"][0][0] = json!(1.0000001);
    assert_eq!(
        compare_group_captures(&expected, &actual, 1.0e-6, 1.0e-6),
        (1, None)
    );
    actual["cases"][0]["output"]["domains"]["point"]["groups"]["out"]["ordered_members"] =
        json!([1, 2]);
    let (_, mismatch) = compare_group_captures(&expected, &actual, 1.0e-6, 1.0e-6);
    assert_eq!(
        mismatch.unwrap()["path"],
        "/output/domains/point/groups/out/ordered_members/0"
    );
}

#[test]
fn performance_contract_rejects_mask_only_benchmarks() {
    let houdini = json!({
        "parameters": { "outputgroup": "out", "numsteps": 2 },
        "input": { "discrete_buffer_serialization": "{}" },
        "output": { "domains": {
            "point": { "count": 3, "groups": { "out": {
                "members": [0, 1, 2], "ordered": false, "ordered_members": []
            } } },
            "vertex": { "count": 0, "groups": {} },
            "edge": { "count": 0, "groups": {} },
            "primitive": { "count": 0, "groups": {} }
        } }
    });
    let mask_only = json!({ "checksum": 3 });
    assert!(performance_contract_mismatch(&houdini, &mask_only).is_some());
    let full_cook = json!({
        "parameters": houdini["parameters"],
        "input_discrete_buffer_serialization": houdini["input"]["discrete_buffer_serialization"],
        "output_contract": performance_output_contract(&houdini["output"]["domains"])
    });
    assert_eq!(performance_contract_mismatch(&houdini, &full_cook), None);
}

#[test]
fn native_performance_receipt_requires_full_cook_fields_for_every_case() {
    let mut cases = GROUP_PERFORMANCE_AXES
        .into_iter()
        .flat_map(|axis| {
            GROUP_PERFORMANCE_PATHS.into_iter().map(move |path| {
                json!({
                    "case_id": format!("performance/{path}/grid_{axis}"),
                    "points": 1,
                    "vertices": 0,
                    "edges": 0,
                    "primitives": 0,
                    "parameters": {},
                    "input_discrete_buffer_serialization": "{}",
                    "output_contract": {},
                    "thread_count": 1,
                    "warmups": 3,
                    "iterations": 15,
                    "memory_scope": "isolated_process_sampled_working_set_peak",
                    "baseline_working_set_bytes": 1,
                    "sampled_peak_working_set_bytes": 1,
                    "sampled_peak_delta_bytes": 0,
                    "process_lifetime_peak_working_set_bytes": 1,
                    "memory_sampler_threads": 1,
                    "memory_sample_interval_ms": 1,
                    "cold_ms": 0.0,
                    "cold_output_contract_exact": true,
                    "timings_ms": vec![0.0; 15],
                    "p50_ms": 0.0,
                    "p95_ms": 0.0,
                    "cold_checksum": 0,
                    "checksum": 0,
                })
            })
        })
        .collect::<Vec<_>>();
    validate_native_group_performance_cases(&cases).unwrap();

    let valid_cases = cases.clone();
    cases[0]["sampled_peak_delta_bytes"] = json!(1);
    assert!(validate_native_group_performance_cases(&cases).is_err());

    cases = valid_cases.clone();
    cases[0]["memory_sample_interval_ms"] = json!(2);
    assert!(validate_native_group_performance_cases(&cases).is_err());

    cases = valid_cases.clone();
    cases[0]["sampled_peak_working_set_bytes"] = json!(2);
    cases[0]["sampled_peak_delta_bytes"] = json!(1);
    assert!(validate_native_group_performance_cases(&cases).is_err());

    cases = valid_cases;
    cases[0].as_object_mut().unwrap().remove("output_contract");
    assert!(validate_native_group_performance_cases(&cases).is_err());
}
