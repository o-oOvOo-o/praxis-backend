use std::time::Duration;

use anyhow::Result;
use app_test_support::McpProcess;
use app_test_support::to_response;
use app_test_support::write_models_cache;
use praxis_app_gateway_protocol::JSONRPCError;
use praxis_app_gateway_protocol::JSONRPCResponse;
use praxis_app_gateway_protocol::Model;
use praxis_app_gateway_protocol::ModelListParams;
use praxis_app_gateway_protocol::ModelListResponse;
use praxis_app_gateway_protocol::RequestId;
use pretty_assertions::assert_eq;
use std::fs;
use tempfile::TempDir;
use tokio::time::timeout;

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(10);
const INVALID_REQUEST_ERROR_CODE: i64 = -32600;

async fn all_visible_models(mcp: &mut McpProcess) -> Result<Vec<Model>> {
    let request_id = mcp
        .send_list_models_request(ModelListParams {
            limit: None,
            cursor: None,
            include_hidden: Some(false),
        })
        .await?;
    let response: JSONRPCResponse = timeout(
        DEFAULT_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(request_id)),
    )
    .await??;
    let response = to_response::<ModelListResponse>(response)?;
    assert!(response.next_cursor.is_none());
    Ok(response.data)
}

#[tokio::test]
async fn list_models_returns_all_models_with_large_limit() -> Result<()> {
    let praxis_home = TempDir::new()?;
    write_models_cache(praxis_home.path())?;
    let mut mcp = McpProcess::new(praxis_home.path()).await?;

    timeout(DEFAULT_TIMEOUT, mcp.initialize()).await??;

    let request_id = mcp
        .send_list_models_request(ModelListParams {
            limit: Some(100),
            cursor: None,
            include_hidden: None,
        })
        .await?;

    let response: JSONRPCResponse = timeout(
        DEFAULT_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(request_id)),
    )
    .await??;

    let ModelListResponse {
        data: items,
        next_cursor,
    } = to_response::<ModelListResponse>(response)?;

    assert!(!items.is_empty());
    assert!(items.iter().all(|model| !model.hidden));
    assert!(items.iter().all(|model| model.default_reasoning_effort
        == praxis_protocol::openai_models::ReasoningEffort::Medium));
    let ids = items
        .iter()
        .map(|model| &model.id)
        .collect::<std::collections::HashSet<_>>();
    assert_eq!(ids.len(), items.len());
    assert!(items.iter().any(|model| model.id == "openai::gpt-6-astra"));
    assert!(next_cursor.is_none());
    Ok(())
}

#[tokio::test]
async fn list_models_includes_hidden_models() -> Result<()> {
    let praxis_home = TempDir::new()?;
    write_models_cache(praxis_home.path())?;
    let mut mcp = McpProcess::new(praxis_home.path()).await?;

    timeout(DEFAULT_TIMEOUT, mcp.initialize()).await??;

    let request_id = mcp
        .send_list_models_request(ModelListParams {
            limit: Some(100),
            cursor: None,
            include_hidden: Some(true),
        })
        .await?;

    let response: JSONRPCResponse = timeout(
        DEFAULT_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(request_id)),
    )
    .await??;

    let ModelListResponse {
        data: items,
        next_cursor,
    } = to_response::<ModelListResponse>(response)?;

    assert!(items.iter().any(|item| item.hidden));
    assert!(next_cursor.is_none());
    Ok(())
}

#[tokio::test]
async fn list_models_pagination_works() -> Result<()> {
    let praxis_home = TempDir::new()?;
    write_models_cache(praxis_home.path())?;
    let mut mcp = McpProcess::new(praxis_home.path()).await?;

    timeout(DEFAULT_TIMEOUT, mcp.initialize()).await??;

    let expected_models = all_visible_models(&mut mcp).await?;
    let mut cursor = None;
    let mut items = Vec::new();

    for _ in 0..expected_models.len() {
        let request_id = mcp
            .send_list_models_request(ModelListParams {
                limit: Some(1),
                cursor: cursor.clone(),
                include_hidden: None,
            })
            .await?;

        let response: JSONRPCResponse = timeout(
            DEFAULT_TIMEOUT,
            mcp.read_stream_until_response_message(RequestId::Integer(request_id)),
        )
        .await??;

        let ModelListResponse {
            data: page_items,
            next_cursor,
        } = to_response::<ModelListResponse>(response)?;

        assert_eq!(page_items.len(), 1);
        items.extend(page_items);

        if let Some(next_cursor) = next_cursor {
            cursor = Some(next_cursor);
        } else {
            assert_eq!(items, expected_models);
            return Ok(());
        }
    }

    panic!(
        "model pagination did not terminate after {} pages",
        expected_models.len()
    );
}

#[tokio::test]
async fn list_models_rejects_invalid_cursor() -> Result<()> {
    let praxis_home = TempDir::new()?;
    write_models_cache(praxis_home.path())?;
    let mut mcp = McpProcess::new(praxis_home.path()).await?;

    timeout(DEFAULT_TIMEOUT, mcp.initialize()).await??;

    let request_id = mcp
        .send_list_models_request(ModelListParams {
            limit: None,
            cursor: Some("invalid".to_string()),
            include_hidden: None,
        })
        .await?;

    let error: JSONRPCError = timeout(
        DEFAULT_TIMEOUT,
        mcp.read_stream_until_error_message(RequestId::Integer(request_id)),
    )
    .await??;

    assert_eq!(error.id, RequestId::Integer(request_id));
    assert_eq!(error.error.code, INVALID_REQUEST_ERROR_CODE);
    assert_eq!(error.error.message, "invalid cursor: invalid");
    Ok(())
}

#[tokio::test]
async fn list_models_includes_current_model_for_non_openai_provider_without_catalog() -> Result<()>
{
    let praxis_home = TempDir::new()?;
    fs::write(
        praxis_home.path().join("config.toml"),
        r#"
model = "glm-5.1"
model_provider = "glm_claude"
approval_policy = "never"
sandbox_mode = "read-only"

[model_providers.glm_claude]
name = "GLM Claude"
base_url = "https://open.bigmodel.cn/api/anthropic"
wire_api = "claude"
supports_websockets = false
"#,
    )?;
    let mut mcp = McpProcess::new(praxis_home.path()).await?;

    timeout(DEFAULT_TIMEOUT, mcp.initialize()).await??;

    let request_id = mcp
        .send_list_models_request(ModelListParams {
            limit: Some(20),
            cursor: None,
            include_hidden: None,
        })
        .await?;

    let response: JSONRPCResponse = timeout(
        DEFAULT_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(request_id)),
    )
    .await??;

    let ModelListResponse {
        data: items,
        next_cursor,
    } = to_response::<ModelListResponse>(response)?;

    let glm = items
        .iter()
        .find(|item| {
            item.model_provider.as_deref() == Some("glm_claude") && item.model == "glm-5.1"
        })
        .expect("current non-openai provider model should be listed");
    assert_eq!(glm.id, "glm_claude::glm-5.1");
    assert_eq!(glm.display_name, "glm-5.1");
    assert!(!glm.hidden);

    let gpt55 = items
        .iter()
        .find(|item| item.model_provider.as_deref() == Some("openai") && item.model == "gpt-5.5")
        .expect("bundled OpenAI GPT-5.5 should be listed for cross-provider selection");
    assert_eq!(gpt55.id, "openai::gpt-5.5");
    assert_eq!(gpt55.display_name, "GPT-5.5");
    assert!(!gpt55.hidden);
    let astra = items
        .iter()
        .find(|item| {
            item.model == "gpt-6-astra" && item.model_provider.as_deref() == Some("openai")
        })
        .expect("GPT-6 Astra should be available through the shared backend catalog");
    assert_eq!(
        astra.default_reasoning_effort,
        praxis_protocol::openai_models::ReasoningEffort::Medium
    );
    assert!(items.iter().all(|item| item.default_reasoning_effort
        == praxis_protocol::openai_models::ReasoningEffort::Medium));
    assert!(next_cursor.is_none());
    Ok(())
}
