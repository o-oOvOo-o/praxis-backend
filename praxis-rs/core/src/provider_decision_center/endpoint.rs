use std::time::Duration;

use http::HeaderMap;
use http::header::HeaderName;
use http::header::HeaderValue;
use praxis_api::Provider as ApiProvider;
use praxis_api::provider::RetryConfig as ApiRetryConfig;
use praxis_login::AuthMode;

use super::ProviderHeaderSource;
use super::is_chatgpt_auth_mode;
use crate::error::PraxisErr;
use crate::error::Result;
use crate::model_provider_info::ModelProviderInfo;

const OPENAI_API_BASE_URL: &str = "https://api.openai.com/v1";
const CHATGPT_HOSTED_RESPONSES_BASE_URL: &str = "https://chatgpt.com/backend-api/codex";

struct ProviderHeaderResolution {
    headers: HeaderMap,
    sources: Vec<ProviderHeaderSource>,
}

pub(super) struct ProviderEndpointResolution {
    pub(super) api_provider: ApiProvider,
    pub(super) header_sources: Vec<ProviderHeaderSource>,
}

pub(super) fn resolve_provider_endpoint(
    provider: &ModelProviderInfo,
    auth_mode: Option<AuthMode>,
) -> Result<ProviderEndpointResolution> {
    let headers = resolve_provider_headers(provider)?;
    let default_base_url = if is_chatgpt_auth_mode(auth_mode) {
        CHATGPT_HOSTED_RESPONSES_BASE_URL
    } else {
        OPENAI_API_BASE_URL
    };
    let base_url = provider
        .base_url
        .clone()
        .unwrap_or_else(|| default_base_url.to_string());
    let api_provider = ApiProvider {
        name: provider.name.clone(),
        base_url,
        query_params: provider.query_params.clone(),
        headers: headers.headers,
        retry: ApiRetryConfig {
            max_attempts: provider.request_max_retries(),
            base_delay: Duration::from_millis(200),
            retry_429: false,
            retry_5xx: true,
            retry_transport: true,
        },
        stream_idle_timeout: provider.stream_idle_timeout(),
    };

    Ok(ProviderEndpointResolution {
        api_provider,
        header_sources: headers.sources,
    })
}

fn resolve_provider_headers(provider: &ModelProviderInfo) -> Result<ProviderHeaderResolution> {
    let capacity = provider
        .http_headers
        .as_ref()
        .map_or(0, |headers| headers.len())
        + provider
            .env_http_headers
            .as_ref()
            .map_or(0, |headers| headers.len());
    let mut headers = HeaderMap::with_capacity(capacity);
    let mut sources = Vec::with_capacity(capacity);

    if let Some(static_headers) = provider.http_headers.as_ref() {
        for (header, value) in static_headers {
            let name = parse_provider_header_name(provider, header)?;
            let mut value = parse_provider_header_value(provider, header, value)?;
            value.set_sensitive(true);
            headers.insert(name, value);
            sources.push(ProviderHeaderSource::Static {
                header: header.clone(),
            });
        }
    }

    if let Some(env_headers) = provider.env_http_headers.as_ref() {
        for (header, env_var) in env_headers {
            let name = parse_provider_header_name(provider, header)?;
            let env_value = std::env::var(env_var)
                .ok()
                .filter(|value| !value.trim().is_empty());
            let value_present = env_value.is_some();
            sources.push(ProviderHeaderSource::Environment {
                header: header.clone(),
                value_present,
            });

            let Some(value) = env_value else {
                continue;
            };
            let mut value = parse_provider_header_value(provider, header, &value)?;
            value.set_sensitive(true);
            headers.insert(name, value);
        }
    }

    Ok(ProviderHeaderResolution { headers, sources })
}

fn parse_provider_header_name(provider: &ModelProviderInfo, header: &str) -> Result<HeaderName> {
    HeaderName::try_from(header).map_err(|err| {
        PraxisErr::InvalidRequest(format!(
            "invalid http header name `{header}` in provider `{}`: {err}",
            provider.name
        ))
    })
}

fn parse_provider_header_value(
    provider: &ModelProviderInfo,
    header: &str,
    value: &str,
) -> Result<HeaderValue> {
    HeaderValue::try_from(value).map_err(|err| {
        PraxisErr::InvalidRequest(format!(
            "invalid http header value for `{header}` in provider `{}`: {err}",
            provider.name
        ))
    })
}
