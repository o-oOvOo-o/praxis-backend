use super::model_output::ensure_safe_json;
use super::redaction::redact_payload_like_text;

#[test]
fn redaction_buckets_payload_like_literals() {
    let raw = "prefix 9090909090909090909090909090909090909090 suffix";
    let projected = redact_payload_like_text(raw);
    assert!(projected.contains("[bucket:payload_like]"));
    assert!(!projected.contains("9090909090909090909090909090909090909090"));
}

#[test]
fn redaction_buckets_secret_like_literals() {
    let raw = "token=abcdef0123456789";
    let projected = redact_payload_like_text(raw);
    assert!(projected.contains("[bucket:secret_like]"));
    assert!(!projected.contains("abcdef0123456789"));
}

#[test]
fn model_output_rejects_raw_decompiler_fields_after_authorization() {
    let output = serde_json::json!({
        "artifact_id": "art_ok",
        "target_hash": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
        "decompiled_source": "int main() { return 0; }"
    });
    let err = ensure_safe_json(&output).expect_err("raw decompiler fields must be rejected");
    assert!(err.to_string().contains("decompiled_source"));
}

#[test]
fn model_output_and_redaction_share_payload_hex_threshold() {
    let raw = "abcdef0123456789abcdef0123456789";
    let projected = redact_payload_like_text(raw);
    assert!(projected.contains("[bucket:payload_like]"));

    let output = serde_json::json!({ "summary": raw });
    let err = ensure_safe_json(&output).expect_err("payload-like hex must be rejected");
    assert!(err.to_string().contains("payload data"));
}
