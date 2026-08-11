fn missing_art_evidence(status: &str, reason: &str, next_commands: Vec<String>) -> Value {
    json!({
        "status": status,
        "passed": false,
        "exact": false,
        "reason": reason,
        "next_commands": next_commands,
    })
}

fn scree_timing_evidence(value: &Value, product_timing: Option<&JsonArtifact>) -> Value {
    let compare_timing = scree_timing_from_value(value, None);
    let Some(product_timing) = product_timing else {
        return compare_timing;
    };
    let product = &product_timing.value;
    let product_summary = scree_timing_from_value(product, Some(product_timing));
    json!({
        "status": "native_product_timing",
        "artifact": artifact_ref(product_timing),
        "source": product.get("source"),
        "resolution": product.get("resolution"),
        "input_map_token": product.get("input_map_token"),
        "product_timing": product_summary,
        "compare_case_timing": compare_timing,
    })
}

fn scree_timing_from_value(value: &Value, artifact: Option<&JsonArtifact>) -> Value {
    let Some(timing) = value.get("native_timing") else {
        return json!({
            "status": "missing_native_repeat_timing",
            "reason": "Scree compare evidence currently proves output correctness but does not expose a native repeat timing summary.",
            "next_command": flywheel_run_command("scree-compare --node Scree --source cone --resolution 32 --scale 0.75 --height 1.35 --density 2 --spread 0.35 --edge 0.7 --seed 11 --epsilon 0.000001 --repeat 100 --direct-bin --run --json"),
        });
    };
    json!({
        "status": "native_repeat_timing",
        "artifact": optional_artifact_ref(artifact),
        "resolution": value.get("resolution"),
        "source": value.get("source"),
        "build_profile": timing.get("build_profile"),
        "elapsed_mode": timing.get("elapsed_mode"),
        "repeat": timing.get("repeat"),
        "sample_count": timing.get("sample_count"),
        "native_avg_elapsed_ms": timing.get("elapsed_ms"),
        "native_min_elapsed_ms": timing.get("min_elapsed_ms"),
        "native_max_elapsed_ms": timing.get("max_elapsed_ms"),
        "profile_repeat": timing.get("profile_repeat"),
        "profiled_elapsed_ms": timing.get("profiled_elapsed_ms"),
        "stage_avg_ms": timing.get("stage_avg_ms"),
        "stage_last_ms": timing.get("stage_last_ms"),
        "sha256": {
            "cratered": timing.get("cratered_sha256_f32"),
            "height": timing.get("height_sha256_f32"),
            "scree": timing.get("scree_sha256_f32"),
            "mask_flow": timing.get("mask_flow_sha256_f32"),
            "mask_normalized": timing.get("mask_normalized_sha256_f32"),
            "mask_spread": timing.get("mask_spread_sha256_f32"),
        },
    })
}

fn stratify_map_evidence(value: Option<&Value>) -> Value {
    let Some(value) = value else {
        return Value::Null;
    };
    json!({
        "status": value.get("status"),
        "sample_count": value.pointer("/metrics/sample_count"),
        "exact_bit_sample_count": value.pointer("/metrics/exact_bit_sample_count"),
        "max_abs_diff": value.pointer("/metrics/max_abs_diff"),
        "sha256": {
            "reference": value.pointer("/metrics/reference_sha256_f32"),
            "candidate": value.pointer("/metrics/candidate_sha256_f32"),
        },
    })
}

fn stratify_timing_evidence(timing: Option<&JsonArtifact>) -> Value {
    let Some(artifact) = timing else {
        return json!({
            "status": "missing_native_repeat_timing",
            "next_command": flywheel_run_command("stratify-compare --node Stratify --resolution 512 --input-map map:rampx:512:0.08:0.92 --native-only --repeat 100 --direct-bin --run --json"),
        });
    };
    let value = &artifact.value;
    json!({
        "status": "native_repeat_timing",
        "artifact": artifact_ref(artifact),
        "resolution": value.get("resolution"),
        "repeat": value.get("repeat"),
        "sample_count": value.get("sample_count"),
        "native_avg_elapsed_ms": value.get("elapsed_ms"),
        "native_min_elapsed_ms": value.get("min_elapsed_ms"),
        "native_max_elapsed_ms": value.get("max_elapsed_ms"),
    })
}
