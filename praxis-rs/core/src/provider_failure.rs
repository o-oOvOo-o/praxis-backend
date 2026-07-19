use std::sync::OnceLock;

use http::StatusCode;
use regex_lite::Regex;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProviderFailureKind {
    ContextOverflow,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProviderFailure {
    pub(crate) kind: ProviderFailureKind,
    pub(crate) status: Option<StatusCode>,
    pub(crate) message: String,
    pub(crate) provider_code: Option<String>,
    pub(crate) context_limit: Option<i64>,
    pub(crate) requested_tokens: Option<i64>,
}

struct ErrorSignature {
    kind: ProviderFailureKind,
    statuses: &'static [StatusCode],
    provider_codes: &'static [&'static str],
    message_patterns: &'static [&'static str],
}

const CONTEXT_OVERFLOW_SIGNATURE: ErrorSignature = ErrorSignature {
    kind: ProviderFailureKind::ContextOverflow,
    statuses: &[StatusCode::BAD_REQUEST, StatusCode::PAYLOAD_TOO_LARGE],
    provider_codes: &["context_length_exceeded", "context_window_exceeded"],
    message_patterns: &[
        "context length",
        "context window",
        "maximum context",
        "max tokens",
        "too many tokens",
        "prompt is too long",
        "input token count",
        "model token limit",
    ],
};

pub(crate) fn classify_http_failure(status: StatusCode, body: &str) -> Option<ProviderFailure> {
    let message = extract_provider_message(body);
    let provider_code = extract_provider_code(body);
    let lower_message = message.to_ascii_lowercase();
    let signature = &CONTEXT_OVERFLOW_SIGNATURE;
    let code_matches = provider_code.as_deref().is_some_and(|code| {
        signature
            .provider_codes
            .iter()
            .any(|candidate| code.eq_ignore_ascii_case(candidate))
    });
    let message_matches = signature
        .message_patterns
        .iter()
        .any(|pattern| lower_message.contains(pattern));
    if !signature.statuses.contains(&status) || (!code_matches && !message_matches) {
        return None;
    }

    let (context_limit, requested_tokens) = extract_context_window_numbers(&message);
    Some(ProviderFailure {
        kind: signature.kind,
        status: Some(status),
        message,
        provider_code,
        context_limit,
        requested_tokens,
    })
}

fn extract_provider_message(body: &str) -> String {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|value| {
            value
                .get("error")
                .and_then(|error| error.get("message"))
                .or_else(|| value.get("message"))
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned)
        })
        .unwrap_or_else(|| body.to_owned())
}

fn extract_provider_code(body: &str) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|value| {
            value
                .get("error")
                .and_then(|error| error.get("code").or_else(|| error.get("type")))
                .or_else(|| value.get("code"))
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned)
        })
}

fn extract_context_window_numbers(message: &str) -> (Option<i64>, Option<i64>) {
    static LIMIT_RE: OnceLock<Regex> = OnceLock::new();
    let regex = LIMIT_RE.get_or_init(|| {
        Regex::new(
            r"(?i)(?:model token limit|maximum(?: number of)? tokens?)\D{0,16}(\d+)(?:\D{0,32}(?:requested|received|got)\D{0,8}(\d+))?",
        )
        .expect("valid context overflow extraction regex")
    });
    let Some(captures) = regex.captures(message) else {
        return (None, None);
    };
    let limit = captures
        .get(1)
        .and_then(|value| value.as_str().parse().ok());
    let requested = captures
        .get(2)
        .and_then(|value| value.as_str().parse().ok());
    (limit, requested)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_context_overflow_and_extracts_provider_limit() {
        let body = serde_json::json!({
            "error": {
                "type": "invalid_request_error",
                "message": "Invalid request: Your request exceeded model token limit: 262144 (requested: 262851)"
            },
            "type": "error"
        })
        .to_string();
        let failure = classify_http_failure(StatusCode::BAD_REQUEST, &body)
            .expect("context overflow failure");
        assert_eq!(failure.kind, ProviderFailureKind::ContextOverflow);
        assert_eq!(failure.context_limit, Some(262_144));
        assert_eq!(failure.requested_tokens, Some(262_851));
    }

    #[test]
    fn leaves_unrelated_bad_requests_unclassified() {
        assert!(
            classify_http_failure(
                StatusCode::BAD_REQUEST,
                r#"{"error":{"message":"invalid tool schema"}}"#,
            )
            .is_none()
        );
    }
}
