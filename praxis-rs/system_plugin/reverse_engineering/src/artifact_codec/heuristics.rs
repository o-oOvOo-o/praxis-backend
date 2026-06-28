pub const MIN_PAYLOAD_HEX_LEN: usize = 32;

pub fn looks_payload_like(input: &str) -> bool {
    if input.contains("\\x") || input.contains("0x9090") {
        return true;
    }
    input.split_whitespace().any(looks_payload_like_token)
}

pub fn looks_secret_like(input: &str) -> bool {
    let lower = input.to_ascii_lowercase();
    lower.contains("password=")
        || lower.contains("token=")
        || lower.contains("apikey")
        || lower.contains("api_key")
}

fn looks_payload_like_token(token: &str) -> bool {
    let clean = token.trim_matches(|c: char| {
        c == ',' || c == ';' || c == '"' || c == '\'' || c == '[' || c == ']'
    });
    clean.len() >= MIN_PAYLOAD_HEX_LEN && clean.chars().all(|c| c.is_ascii_hexdigit())
}
