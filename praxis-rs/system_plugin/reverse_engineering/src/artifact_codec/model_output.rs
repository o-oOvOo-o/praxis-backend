use crate::ReverseError;
use crate::artifact_codec::heuristics::looks_payload_like;

const MAX_MODEL_STRING_BYTES: usize = 8192;

pub fn ensure_safe_json(value: &serde_json::Value) -> Result<(), ReverseError> {
    inspect_value(value, None)
}

fn inspect_value(value: &serde_json::Value, key: Option<&str>) -> Result<(), ReverseError> {
    match value {
        serde_json::Value::Object(map) => {
            for (child_key, child_value) in map {
                if is_raw_payload_key(child_key) {
                    return Err(ReverseError::Codec(format!(
                        "model output attempted to expose raw reverse-engineering field `{child_key}`"
                    )));
                }
                inspect_value(child_value, Some(child_key))?;
            }
        }
        serde_json::Value::Array(values) => {
            for child in values {
                inspect_value(child, key)?;
            }
        }
        serde_json::Value::String(text) => inspect_string(key, text)?,
        serde_json::Value::Null | serde_json::Value::Bool(_) | serde_json::Value::Number(_) => {}
    }
    Ok(())
}

fn inspect_string(key: Option<&str>, text: &str) -> Result<(), ReverseError> {
    if text.len() > MAX_MODEL_STRING_BYTES {
        return Err(ReverseError::Codec(format!(
            "model output string `{}` exceeds codec projection budget",
            key.unwrap_or("<unknown>")
        )));
    }
    if is_hash_key(key) {
        return Ok(());
    }
    if looks_payload_like(text) {
        return Err(ReverseError::Codec(format!(
            "model output string `{}` still looks like raw payload data",
            key.unwrap_or("<unknown>")
        )));
    }
    Ok(())
}

fn is_raw_payload_key(key: &str) -> bool {
    matches!(
        key,
        "raw"
            | "raw_bytes"
            | "raw_text"
            | "raw_output"
            | "decompiled_source"
            | "decompiler_output"
            | "asm_text"
            | "pcode_text"
            | "shellcode"
            | "payload"
    )
}

fn is_hash_key(key: Option<&str>) -> bool {
    key.is_some_and(|key| key == "target_hash" || key == "sha256" || key.ends_with("_hash"))
}
