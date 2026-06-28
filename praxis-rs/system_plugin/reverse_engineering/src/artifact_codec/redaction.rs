use crate::ReverseError;
use crate::artifact_codec::heuristics::looks_payload_like;
use crate::artifact_codec::heuristics::looks_secret_like;
use std::io::Read;
use std::path::Path;

const MAX_SAFE_PREVIEW_BYTES: usize = 2048;

pub fn safe_text_projection(path: &Path) -> Result<String, ReverseError> {
    let mut file = std::fs::File::open(path).map_err(|err| ReverseError::io(path, err))?;
    let mut bytes = vec![0_u8; MAX_SAFE_PREVIEW_BYTES];
    let read = file
        .read(&mut bytes)
        .map_err(|err| ReverseError::io(path, err))?;
    bytes.truncate(read);
    let text = String::from_utf8_lossy(&bytes);
    Ok(redact_payload_like_text(&text))
}

pub fn redact_payload_like_text(input: &str) -> String {
    let mut out = String::with_capacity(input.len().min(MAX_SAFE_PREVIEW_BYTES));
    for token in input.split_whitespace().take(128) {
        if looks_payload_like(token) {
            out.push_str("[bucket:payload_like]");
        } else if looks_secret_like(token) {
            out.push_str("[bucket:secret_like]");
        } else {
            out.push_str(token);
        }
        out.push(' ');
    }
    out.trim().to_string()
}
