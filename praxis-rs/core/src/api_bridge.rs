use base64::Engine;
use chrono::DateTime;
use chrono::Utc;
use http::HeaderMap;
use praxis_api::AuthProvider as ApiAuthProvider;
use praxis_api::AuthScheme as ApiAuthScheme;
use praxis_api::TransportError;
use praxis_api::error::ApiError;
use praxis_api::rate_limits::parse_promo_message;
use praxis_api::rate_limits::parse_rate_limit_for_limit;
use praxis_login::token_data::PlanType;
use serde::Deserialize;
use serde_json::Value;
use std::fmt;
use std::sync::Arc;
use zeroize::Zeroizing;

use crate::error::PraxisErr;
use crate::error::RetryLimitReachedError;
use crate::error::UnexpectedResponseError;
use crate::error::UsageLimitReachedError;

pub(crate) fn map_api_error(err: ApiError) -> PraxisErr {
    match err {
        ApiError::ContextWindowExceeded => PraxisErr::ContextWindowExceeded,
        ApiError::QuotaExceeded => PraxisErr::QuotaExceeded,
        ApiError::UsageNotIncluded => PraxisErr::UsageNotIncluded,
        ApiError::Retryable { message, delay } => PraxisErr::Stream(message, delay),
        ApiError::Stream(msg) => PraxisErr::Stream(msg, None),
        ApiError::ServerOverloaded => PraxisErr::ServerOverloaded,
        ApiError::Api { status, message } => PraxisErr::UnexpectedStatus(UnexpectedResponseError {
            status,
            body: message,
            url: None,
            cf_ray: None,
            request_id: None,
            identity_authorization_error: None,
            identity_error_code: None,
        }),
        ApiError::InvalidRequest { message } => PraxisErr::InvalidRequest(message),
        ApiError::Transport(transport) => match transport {
            TransportError::Http {
                status,
                url,
                headers,
                body,
            } => {
                let body_text = body.unwrap_or_default();

                if status == http::StatusCode::SERVICE_UNAVAILABLE
                    && let Ok(value) = serde_json::from_str::<serde_json::Value>(&body_text)
                    && matches!(
                        value
                            .get("error")
                            .and_then(|error| error.get("code"))
                            .and_then(serde_json::Value::as_str),
                        Some("server_is_overloaded" | "slow_down")
                    )
                {
                    return PraxisErr::ServerOverloaded;
                }

                if status == http::StatusCode::BAD_REQUEST {
                    if body_text
                        .contains("The image data you provided does not represent a valid image")
                    {
                        PraxisErr::InvalidImageRequest()
                    } else {
                        PraxisErr::InvalidRequest(body_text)
                    }
                } else if status == http::StatusCode::INTERNAL_SERVER_ERROR {
                    PraxisErr::InternalServerError
                } else if status == http::StatusCode::TOO_MANY_REQUESTS {
                    if let Ok(err) = serde_json::from_str::<UsageErrorResponse>(&body_text) {
                        if err.error.error_type.as_deref() == Some("usage_limit_reached") {
                            let limit_id = extract_header(headers.as_ref(), ACTIVE_LIMIT_HEADER);
                            let rate_limits = headers.as_ref().and_then(|map| {
                                parse_rate_limit_for_limit(map, limit_id.as_deref())
                            });
                            let promo_message = headers.as_ref().and_then(parse_promo_message);
                            let resets_at = err
                                .error
                                .resets_at
                                .and_then(|seconds| DateTime::<Utc>::from_timestamp(seconds, 0));
                            return PraxisErr::UsageLimitReached(UsageLimitReachedError {
                                plan_type: err.error.plan_type,
                                resets_at,
                                rate_limits: rate_limits.map(Box::new),
                                promo_message,
                            });
                        } else if err.error.error_type.as_deref() == Some("usage_not_included") {
                            return PraxisErr::UsageNotIncluded;
                        }
                    }

                    PraxisErr::RetryLimit(RetryLimitReachedError {
                        status,
                        request_id: extract_request_tracking_id(headers.as_ref()),
                    })
                } else {
                    PraxisErr::UnexpectedStatus(UnexpectedResponseError {
                        status,
                        body: body_text,
                        url,
                        cf_ray: extract_header(headers.as_ref(), CF_RAY_HEADER),
                        request_id: extract_request_id(headers.as_ref()),
                        identity_authorization_error: extract_header(
                            headers.as_ref(),
                            X_OPENAI_AUTHORIZATION_ERROR_HEADER,
                        ),
                        identity_error_code: extract_x_error_json_code(headers.as_ref()),
                    })
                }
            }
            TransportError::RetryLimit => PraxisErr::RetryLimit(RetryLimitReachedError {
                status: http::StatusCode::INTERNAL_SERVER_ERROR,
                request_id: None,
            }),
            TransportError::Timeout => PraxisErr::Timeout,
            TransportError::Network(msg) | TransportError::Build(msg) => {
                PraxisErr::Stream(msg, None)
            }
        },
        ApiError::RateLimit(msg) => PraxisErr::Stream(msg, None),
    }
}

const ACTIVE_LIMIT_HEADER: &str = "x-praxis-active-limit";
const REQUEST_ID_HEADER: &str = "x-request-id";
const OAI_REQUEST_ID_HEADER: &str = "x-oai-request-id";
const CF_RAY_HEADER: &str = "cf-ray";
const X_OPENAI_AUTHORIZATION_ERROR_HEADER: &str = "x-openai-authorization-error";
const X_ERROR_JSON_HEADER: &str = "x-error-json";

#[cfg(test)]
#[path = "api_bridge_tests.rs"]
mod tests;

fn extract_request_tracking_id(headers: Option<&HeaderMap>) -> Option<String> {
    extract_request_id(headers).or_else(|| extract_header(headers, CF_RAY_HEADER))
}

fn extract_request_id(headers: Option<&HeaderMap>) -> Option<String> {
    extract_header(headers, REQUEST_ID_HEADER)
        .or_else(|| extract_header(headers, OAI_REQUEST_ID_HEADER))
}

fn extract_header(headers: Option<&HeaderMap>, name: &str) -> Option<String> {
    headers.and_then(|map| {
        map.get(name)
            .and_then(|value| value.to_str().ok())
            .map(str::to_string)
    })
}

fn extract_x_error_json_code(headers: Option<&HeaderMap>) -> Option<String> {
    let encoded = extract_header(headers, X_ERROR_JSON_HEADER)?;
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .ok()?;
    let parsed = serde_json::from_slice::<Value>(&decoded).ok()?;
    parsed
        .get("error")
        .and_then(|error| error.get("code"))
        .and_then(Value::as_str)
        .map(str::to_string)
}

#[derive(Debug, Deserialize)]
struct UsageErrorResponse {
    error: UsageErrorBody,
}

#[derive(Debug, Deserialize)]
struct UsageErrorBody {
    #[serde(rename = "type")]
    error_type: Option<String>,
    plan_type: Option<PlanType>,
    resets_at: Option<i64>,
}

enum CoreAuthSecret {
    Owned(Zeroizing<String>),
    ProviderApiKey(praxis_login::ProviderApiKey),
    AnthropicOauth(praxis_login::AnthropicOauthAccessToken),
}

impl CoreAuthSecret {
    fn expose_secret(&self) -> &str {
        match self {
            Self::Owned(secret) => secret.as_str(),
            Self::ProviderApiKey(secret) => secret.expose_secret(),
            Self::AnthropicOauth(secret) => secret.expose_secret(),
        }
    }
}

impl fmt::Debug for CoreAuthSecret {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("CoreAuthSecret([REDACTED])")
    }
}

#[derive(Clone, Default)]
pub(crate) struct CoreAuthProvider {
    token: Option<Arc<CoreAuthSecret>>,
    auth_scheme: ApiAuthScheme,
    account_id: Option<String>,
    request_profile: ProviderRequestAuthProfile,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum ProviderRequestAuthProfile {
    #[default]
    Standard,
    AnthropicOauth,
}

impl CoreAuthProvider {
    pub(crate) fn new(token: Option<String>, account_id: Option<String>) -> Self {
        Self {
            token: token.map(|token| Arc::new(CoreAuthSecret::Owned(Zeroizing::new(token)))),
            auth_scheme: ApiAuthScheme::Bearer,
            account_id,
            request_profile: ProviderRequestAuthProfile::Standard,
        }
    }

    pub(crate) fn from_provider_api_key(
        api_key: praxis_login::ProviderApiKey,
        wire_api: crate::model_provider_info::WireApi,
    ) -> Self {
        Self {
            token: Some(Arc::new(CoreAuthSecret::ProviderApiKey(api_key))),
            auth_scheme: auth_scheme_for_wire(wire_api),
            account_id: None,
            request_profile: ProviderRequestAuthProfile::Standard,
        }
    }

    pub(crate) fn from_anthropic_oauth(token: praxis_login::AnthropicOauthAccessToken) -> Self {
        Self {
            token: Some(Arc::new(CoreAuthSecret::AnthropicOauth(token))),
            auth_scheme: ApiAuthScheme::Bearer,
            account_id: None,
            request_profile: ProviderRequestAuthProfile::AnthropicOauth,
        }
    }

    pub(crate) fn is_anthropic_oauth(&self) -> bool {
        self.request_profile == ProviderRequestAuthProfile::AnthropicOauth
    }

    pub(crate) fn bearer_token_value(&self) -> Option<&str> {
        self.token.as_deref().map(CoreAuthSecret::expose_secret)
    }

    pub(crate) fn auth_header_attached(&self) -> bool {
        self.bearer_token_value()
            .is_some_and(|token| http::HeaderValue::from_str(token).is_ok())
    }

    pub(crate) fn auth_header_name(&self) -> Option<&'static str> {
        self.auth_header_attached()
            .then_some(match self.auth_scheme {
                ApiAuthScheme::Bearer => "authorization",
                ApiAuthScheme::XApiKey => "x-api-key",
            })
    }

    #[cfg(test)]
    pub(crate) fn for_test(token: Option<&str>, account_id: Option<&str>) -> Self {
        Self::new(token.map(str::to_string), account_id.map(str::to_string))
    }

    #[cfg(test)]
    pub(crate) fn for_test_claude_api_key(token: Option<&str>) -> Self {
        token.map_or_else(Self::default, |token| {
            let api_key = praxis_login::ProviderApiKey::new(token.to_string())
                .expect("test Claude API key must be valid");
            Self::from_provider_api_key(api_key, crate::model_provider_info::WireApi::Claude)
        })
    }
}

fn auth_scheme_for_wire(wire_api: crate::model_provider_info::WireApi) -> ApiAuthScheme {
    match wire_api {
        crate::model_provider_info::WireApi::Claude => ApiAuthScheme::XApiKey,
        crate::model_provider_info::WireApi::Responses
        | crate::model_provider_info::WireApi::OpenAiCompat => ApiAuthScheme::Bearer,
    }
}

impl ApiAuthProvider for CoreAuthProvider {
    fn bearer_token(&self) -> Option<&str> {
        self.bearer_token_value()
    }

    fn auth_scheme(&self) -> ApiAuthScheme {
        self.auth_scheme
    }

    fn account_id(&self) -> Option<&str> {
        self.account_id.as_deref()
    }
}
