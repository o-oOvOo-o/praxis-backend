use super::*;

pub(crate) async fn stream_claude_unary(
    api_provider: Provider,
    api_auth: CoreAuthProvider,
    provider_info: &ModelProviderInfo,
    prompt: &Prompt,
    model_info: &ModelInfo,
    effort: Option<ReasoningEffortConfig>,
) -> Result<ResponseStream> {
    let mut request_body = build_claude_request(prompt, model_info, provider_info, effort, true)?;
    if api_auth.is_anthropic_oauth() {
        apply_claude_oauth_request_profile(&mut request_body)?;
    }
    let response = send_request(
        &api_provider,
        &api_auth,
        build_claude_endpoint_path(&api_provider),
        &request_body,
        RequestFamily::Claude,
        ProviderTransportPolicy::SystemProxy,
    )
    .await?;

    if response_is_sse(&response) {
        return Ok(spawn_claude_sse_stream(
            response,
            api_provider.stream_idle_timeout,
        ));
    }

    let response_json = read_claude_json_response(response).await?;
    build_response_stream(parse_claude_response(response_json)?)
}

pub(crate) async fn stream_common_unary(
    api_provider: Provider,
    api_auth: CoreAuthProvider,
    provider_info: &ModelProviderInfo,
    prompt: &Prompt,
    model_info: &ModelInfo,
    effort: Option<ReasoningEffortConfig>,
) -> Result<ResponseStream> {
    stream_common_unary_with_mode(
        api_provider,
        api_auth,
        provider_info,
        prompt,
        model_info,
        effort,
        true,
    )
    .await
}

async fn stream_common_unary_with_mode(
    api_provider: Provider,
    api_auth: CoreAuthProvider,
    provider_info: &ModelProviderInfo,
    prompt: &Prompt,
    model_info: &ModelInfo,
    effort: Option<ReasoningEffortConfig>,
    stream: bool,
) -> Result<ResponseStream> {
    let common_compat = CommonRequestCompat::from_provider_and_model(provider_info, model_info);
    let thinking_policy = CommonThinkingPolicy::from_format(common_compat.thinking_format);
    let request_body = build_common_request(prompt, model_info, provider_info, effort, stream)?;
    let response = send_request(
        &api_provider,
        &api_auth,
        build_common_endpoint_path(&api_provider),
        &request_body,
        RequestFamily::Common,
        ProviderTransportPolicy::from_model_provider(provider_info),
    )
    .await?;

    if response_is_sse(&response) {
        return Ok(spawn_common_sse_stream(
            response,
            api_provider.stream_idle_timeout,
            thinking_policy,
        ));
    }

    let response_json = read_json_response(response).await?;
    build_response_stream(parse_common_response(response_json, thinking_policy)?)
}

#[derive(Clone, Copy)]
pub(super) enum RequestFamily {
    Claude,
    Common,
}

#[derive(Clone, Copy)]
pub(super) enum ProviderTransportPolicy {
    SystemProxy,
    Direct,
}

impl ProviderTransportPolicy {
    pub(super) fn from_model_provider(provider_info: &ModelProviderInfo) -> Self {
        if provider_info.is_openai() || provider_info.has_command_auth() {
            Self::SystemProxy
        } else {
            Self::Direct
        }
    }
}

pub(super) struct ParsedProviderResponse {
    pub(super) response_id: String,
    pub(super) token_usage: Option<TokenUsage>,
    pub(super) items: Vec<ResponseItem>,
}

pub(super) async fn send_request(
    api_provider: &Provider,
    api_auth: &CoreAuthProvider,
    endpoint_path: &str,
    request_body: &Value,
    family: RequestFamily,
    transport_policy: ProviderTransportPolicy,
) -> Result<reqwest::Response> {
    let client = match (family, transport_policy) {
        (RequestFamily::Claude, ProviderTransportPolicy::SystemProxy) => {
            try_build_reqwest_client_without_redirects().map_err(|_| {
                PraxisErr::Stream(
                    "failed to build secure Claude HTTP client".to_string(),
                    None,
                )
            })?
        }
        (RequestFamily::Claude, ProviderTransportPolicy::Direct) => {
            try_build_direct_reqwest_client_without_redirects().map_err(|_| {
                PraxisErr::Stream(
                    "failed to build secure Claude HTTP client".to_string(),
                    None,
                )
            })?
        }
        (RequestFamily::Common, ProviderTransportPolicy::SystemProxy) => build_reqwest_client(),
        (RequestFamily::Common, ProviderTransportPolicy::Direct) => build_direct_reqwest_client(),
    };
    let url = api_provider.url_for_path(endpoint_path);
    let headers = build_request_headers(api_provider, api_auth, family)?;

    let response = client
        .post(url.clone())
        .headers(headers)
        .json(request_body)
        .send()
        .await
        .map_err(|err| match family {
            RequestFamily::Claude => map_claude_reqwest_error(err),
            RequestFamily::Common => map_reqwest_error(err),
        })?;

    let status = response.status();
    if !status.is_success() {
        let (response_url, response_headers, body) = match family {
            RequestFamily::Claude => (
                None,
                None,
                Some(format!(
                    "Anthropic API request failed with HTTP status {status}"
                )),
            ),
            RequestFamily::Common => {
                let response_url = response.url().to_string();
                let response_headers = response.headers().clone();
                let body = response.text().await.map_err(map_reqwest_error)?;
                (Some(response_url), Some(response_headers), Some(body))
            }
        };
        let transport = TransportError::Http {
            status,
            url: response_url,
            headers: response_headers,
            body,
        };
        return Err(map_api_error(ApiError::Transport(transport)));
    }

    Ok(response)
}

pub(super) async fn read_claude_json_response(response: reqwest::Response) -> Result<Value> {
    let body = response.text().await.map_err(map_claude_reqwest_error)?;
    serde_json::from_str(&body).map_err(|_| {
        PraxisErr::InvalidRequest("provider returned invalid Claude JSON response".to_string())
    })
}

pub(super) async fn read_json_response(response: reqwest::Response) -> Result<Value> {
    let body = response.text().await.map_err(map_reqwest_error)?;
    serde_json::from_str(&body).map_err(PraxisErr::from)
}

pub(super) fn map_claude_reqwest_error(err: reqwest::Error) -> PraxisErr {
    if err.is_timeout() {
        return map_api_error(ApiError::Transport(TransportError::Timeout));
    }
    map_api_error(ApiError::Transport(TransportError::Network(
        "claude request transport failed".to_string(),
    )))
}

pub(super) fn build_request_headers(
    api_provider: &Provider,
    api_auth: &CoreAuthProvider,
    family: RequestFamily,
) -> Result<HeaderMap> {
    let mut headers = api_provider.headers.clone();

    match family {
        RequestFamily::Claude => {
            insert_header_if_missing(&mut headers, "anthropic-version", ANTHROPIC_API_VERSION)?;
            if api_auth.is_anthropic_oauth() {
                insert_header_if_missing(
                    &mut headers,
                    "anthropic-beta",
                    "claude-code-20250219,oauth-2025-04-20",
                )?;
                insert_header_if_missing(
                    &mut headers,
                    "anthropic-dangerous-direct-browser-access",
                    "true",
                )?;
                insert_header_if_missing(&mut headers, "user-agent", "claude-cli/2.1.207")?;
                insert_header_if_missing(&mut headers, "x-app", "cli")?;
            }
            attach_token_if_missing(&mut headers, api_auth)?;
        }
        RequestFamily::Common => {
            attach_token_if_missing(&mut headers, api_auth)?;
        }
    }

    Ok(headers)
}

pub(super) fn attach_token_if_missing(
    headers: &mut HeaderMap,
    api_auth: &CoreAuthProvider,
) -> Result<()> {
    let Some(token) = api_auth.bearer_token() else {
        return Ok(());
    };

    match api_auth.auth_scheme() {
        praxis_api::AuthScheme::Bearer => {
            if headers.contains_key(AUTHORIZATION) {
                return Ok(());
            }
            let encoded = zeroize::Zeroizing::new(format!("Bearer {token}"));
            let mut value = HeaderValue::from_str(encoded.as_str()).map_err(|err| {
                PraxisErr::InvalidRequest(format!("failed to encode bearer token header: {err}"))
            })?;
            value.set_sensitive(true);
            headers.insert(AUTHORIZATION, value);
        }
        praxis_api::AuthScheme::XApiKey => {
            if headers.contains_key("x-api-key") {
                return Ok(());
            }
            let mut value = HeaderValue::from_str(token).map_err(|err| {
                PraxisErr::InvalidRequest(format!(
                    "failed to encode Anthropic API key header: {err}"
                ))
            })?;
            value.set_sensitive(true);
            headers.insert("x-api-key", value);
        }
    }

    Ok(())
}

pub(super) fn insert_header_if_missing(
    headers: &mut HeaderMap,
    key: &str,
    value: &str,
) -> Result<()> {
    if headers.contains_key(key) {
        return Ok(());
    }
    let header_name: http::header::HeaderName = key.parse().map_err(|err| {
        PraxisErr::InvalidRequest(format!(
            "failed to parse provider header name `{key}`: {err}"
        ))
    })?;
    let header_value = HeaderValue::from_str(value).map_err(|err| {
        PraxisErr::InvalidRequest(format!(
            "failed to parse provider header `{key}` value: {err}"
        ))
    })?;
    headers.insert(header_name, header_value);
    Ok(())
}

pub(super) fn build_claude_endpoint_path(api_provider: &Provider) -> &'static str {
    let base = api_provider
        .base_url
        .trim_end_matches('/')
        .to_ascii_lowercase();
    if base.ends_with("/messages") {
        ""
    } else if base.ends_with("/v1") {
        "messages"
    } else {
        "v1/messages"
    }
}

pub(super) fn build_common_endpoint_path(api_provider: &Provider) -> &'static str {
    let base = api_provider
        .base_url
        .trim_end_matches('/')
        .to_ascii_lowercase();
    if base.ends_with("/chat/completions") {
        ""
    } else if base.ends_with("/v1") {
        "chat/completions"
    } else {
        "v1/chat/completions"
    }
}
