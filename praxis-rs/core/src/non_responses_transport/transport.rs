use super::*;

pub(crate) async fn stream_claude_unary(
    api_provider: Provider,
    api_auth: CoreAuthProvider,
    prompt: &Prompt,
    model_info: &ModelInfo,
) -> Result<ResponseStream> {
    let request_body = build_claude_request(prompt, model_info, true)?;
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

    let response_json = read_json_response(response).await?;
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
    let common_compat = CommonRequestCompat::from_provider_and_model(provider_info, model_info);
    let thinking_policy = CommonThinkingPolicy::from_format(common_compat.thinking_format);
    let request_body = build_common_request(prompt, model_info, provider_info, effort, true)?;
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
    let client = match transport_policy {
        ProviderTransportPolicy::SystemProxy => build_reqwest_client(),
        ProviderTransportPolicy::Direct => build_direct_reqwest_client(),
    };
    let url = api_provider.url_for_path(endpoint_path);
    let headers = build_request_headers(api_provider, api_auth, family)?;

    let response = client
        .post(url.clone())
        .headers(headers)
        .json(request_body)
        .send()
        .await
        .map_err(map_reqwest_error)?;

    let status = response.status();
    if !status.is_success() {
        let response_url = response.url().to_string();
        let response_headers = response.headers().clone();
        let body = response.text().await.map_err(map_reqwest_error)?;
        let transport = TransportError::Http {
            status,
            url: Some(response_url),
            headers: Some(response_headers),
            body: Some(body),
        };
        return Err(map_api_error(ApiError::Transport(transport)));
    }

    Ok(response)
}

pub(super) async fn read_json_response(response: reqwest::Response) -> Result<Value> {
    let body = response.text().await.map_err(map_reqwest_error)?;
    serde_json::from_str(&body).map_err(PraxisErr::from)
}

pub(super) fn build_request_headers(
    api_provider: &Provider,
    api_auth: &CoreAuthProvider,
    family: RequestFamily,
) -> Result<HeaderMap> {
    let mut headers = api_provider.headers.clone();

    match family {
        RequestFamily::Claude => {
            insert_header_if_missing(&mut headers, "anthropic-version", CLAUDE_API_VERSION)?;
            attach_token_if_missing(&mut headers, api_auth, TokenHeaderMode::ClaudeApiKey)?;
        }
        RequestFamily::Common => {
            attach_token_if_missing(&mut headers, api_auth, TokenHeaderMode::Bearer)?;
        }
    }

    Ok(headers)
}

pub(super) enum TokenHeaderMode {
    Bearer,
    ClaudeApiKey,
}

pub(super) fn attach_token_if_missing(
    headers: &mut HeaderMap,
    api_auth: &CoreAuthProvider,
    mode: TokenHeaderMode,
) -> Result<()> {
    let Some(token) = api_auth.bearer_token() else {
        return Ok(());
    };

    if headers.contains_key(AUTHORIZATION) || headers.contains_key("x-api-key") {
        return Ok(());
    }

    match mode {
        TokenHeaderMode::Bearer => {
            let value = HeaderValue::from_str(&format!("Bearer {token}")).map_err(|err| {
                PraxisErr::InvalidRequest(format!("failed to encode bearer token header: {err}"))
            })?;
            headers.insert(AUTHORIZATION, value);
        }
        TokenHeaderMode::ClaudeApiKey => {
            insert_header_if_missing(headers, "x-api-key", &token)?;
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
