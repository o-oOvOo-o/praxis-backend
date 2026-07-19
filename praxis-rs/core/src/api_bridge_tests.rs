use super::*;
use base64::Engine;
use pretty_assertions::assert_eq;

#[test]
fn map_api_error_maps_server_overloaded() {
    let err = map_api_error(ApiError::ServerOverloaded);
    assert!(matches!(err, PraxisErr::ServerOverloaded));
}

#[test]
fn map_api_error_maps_server_overloaded_from_503_body() {
    let body = serde_json::json!({
        "error": {
            "code": "server_is_overloaded"
        }
    })
    .to_string();
    let err = map_api_error(ApiError::Transport(TransportError::Http {
        status: http::StatusCode::SERVICE_UNAVAILABLE,
        url: Some("http://example.com/v1/responses".to_string()),
        headers: None,
        body: Some(body),
    }));

    assert!(matches!(err, PraxisErr::ServerOverloaded));
}

#[test]
fn map_api_error_maps_usage_limit_limit_name_header() {
    let mut headers = HeaderMap::new();
    headers.insert(
        ACTIVE_LIMIT_HEADER,
        http::HeaderValue::from_static("praxis_other"),
    );
    headers.insert(
        "x-praxis-other-limit-name",
        http::HeaderValue::from_static("praxis_other"),
    );
    let body = serde_json::json!({
        "error": {
            "type": "usage_limit_reached",
            "plan_type": "pro",
        }
    })
    .to_string();
    let err = map_api_error(ApiError::Transport(TransportError::Http {
        status: http::StatusCode::TOO_MANY_REQUESTS,
        url: Some("http://example.com/v1/responses".to_string()),
        headers: Some(headers),
        body: Some(body),
    }));

    let PraxisErr::UsageLimitReached(usage_limit) = err else {
        panic!("expected PraxisErr::UsageLimitReached, got {err:?}");
    };
    assert_eq!(
        usage_limit
            .rate_limits
            .as_ref()
            .and_then(|snapshot| snapshot.limit_name.as_deref()),
        Some("praxis_other")
    );
}

#[test]
fn map_api_error_does_not_fallback_limit_name_to_limit_id() {
    let mut headers = HeaderMap::new();
    headers.insert(
        ACTIVE_LIMIT_HEADER,
        http::HeaderValue::from_static("praxis_other"),
    );
    let body = serde_json::json!({
        "error": {
            "type": "usage_limit_reached",
            "plan_type": "pro",
        }
    })
    .to_string();
    let err = map_api_error(ApiError::Transport(TransportError::Http {
        status: http::StatusCode::TOO_MANY_REQUESTS,
        url: Some("http://example.com/v1/responses".to_string()),
        headers: Some(headers),
        body: Some(body),
    }));

    let PraxisErr::UsageLimitReached(usage_limit) = err else {
        panic!("expected PraxisErr::UsageLimitReached, got {err:?}");
    };
    assert_eq!(
        usage_limit
            .rate_limits
            .as_ref()
            .and_then(|snapshot| snapshot.limit_name.as_deref()),
        None
    );
}

#[test]
fn map_api_error_extracts_identity_auth_details_from_headers() {
    let mut headers = HeaderMap::new();
    headers.insert(REQUEST_ID_HEADER, http::HeaderValue::from_static("req-401"));
    headers.insert(CF_RAY_HEADER, http::HeaderValue::from_static("ray-401"));
    headers.insert(
        X_OPENAI_AUTHORIZATION_ERROR_HEADER,
        http::HeaderValue::from_static("missing_authorization_header"),
    );
    let x_error_json =
        base64::engine::general_purpose::STANDARD.encode(r#"{"error":{"code":"token_expired"}}"#);
    headers.insert(
        X_ERROR_JSON_HEADER,
        http::HeaderValue::from_str(&x_error_json).expect("valid x-error-json header"),
    );

    let err = map_api_error(ApiError::Transport(TransportError::Http {
        status: http::StatusCode::UNAUTHORIZED,
        url: Some("https://chatgpt.com/backend-api/codex/models".to_string()),
        headers: Some(headers),
        body: Some(r#"{"detail":"Unauthorized"}"#.to_string()),
    }));

    let PraxisErr::UnexpectedStatus(err) = err else {
        panic!("expected PraxisErr::UnexpectedStatus, got {err:?}");
    };
    assert_eq!(err.request_id.as_deref(), Some("req-401"));
    assert_eq!(err.cf_ray.as_deref(), Some("ray-401"));
    assert_eq!(
        err.identity_authorization_error.as_deref(),
        Some("missing_authorization_header")
    );
    assert_eq!(err.identity_error_code.as_deref(), Some("token_expired"));
}

#[test]
fn core_auth_provider_reports_when_auth_header_will_attach() {
    let auth = CoreAuthProvider::for_test(Some("access-token"), None);

    assert!(auth.auth_header_attached());
    assert_eq!(auth.auth_header_name(), Some("authorization"));
}

#[test]
fn map_api_error_maps_provider_429_with_retry_metadata() {
    let mut headers = HeaderMap::new();
    headers.insert(
        REQUEST_ID_HEADER,
        http::HeaderValue::from_static("req-kimi"),
    );
    headers.insert(
        X_TRACE_ID_HEADER,
        http::HeaderValue::from_static("trace-kimi"),
    );
    headers.insert(RETRY_AFTER_HEADER, http::HeaderValue::from_static("12"));
    let body = serde_json::json!({
        "error": {
            "type": "rate_limit_error",
            "message": "Concurrent request limit reached"
        }
    })
    .to_string();

    let err = map_api_error(ApiError::Transport(TransportError::Http {
        status: http::StatusCode::TOO_MANY_REQUESTS,
        url: Some("https://api.kimi.com/coding/v1/messages".to_string()),
        headers: Some(headers),
        body: Some(body),
    }));

    let PraxisErr::ProviderRateLimited(rate_limit) = err else {
        panic!("expected PraxisErr::ProviderRateLimited, got {err:?}");
    };
    assert_eq!(rate_limit.message, "Concurrent request limit reached");
    assert_eq!(rate_limit.request_id.as_deref(), Some("req-kimi"));
    assert_eq!(rate_limit.trace_id.as_deref(), Some("trace-kimi"));
    assert_eq!(
        rate_limit.retry_after,
        Some(std::time::Duration::from_secs(12))
    );
}

#[test]
fn map_api_error_recognizes_structured_context_overflow_from_http_400() {
    let body = serde_json::json!({
        "error": {
            "type": "invalid_request_error",
            "message": "Invalid request: Your request exceeded model token limit: 262144 (requested: 262851)"
        },
        "type": "error"
    })
    .to_string();
    let err = map_api_error(ApiError::Transport(TransportError::Http {
        status: http::StatusCode::BAD_REQUEST,
        url: Some("https://provider.example/v1/messages".to_string()),
        headers: None,
        body: Some(body),
    }));

    let PraxisErr::ContextWindowExceeded(overflow) = err else {
        panic!("expected context overflow, got {err:?}");
    };
    assert_eq!(overflow.context_limit, Some(262_144));
    assert_eq!(overflow.requested_tokens, Some(262_851));
}

#[test]
fn core_auth_provider_reports_claude_api_key_header() {
    let auth = CoreAuthProvider::for_test_claude_api_key(Some("claude-key"));

    assert!(auth.auth_header_attached());
    assert_eq!(auth.auth_header_name(), Some("x-api-key"));
}
