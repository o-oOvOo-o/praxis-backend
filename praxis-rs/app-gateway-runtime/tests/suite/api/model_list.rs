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
use praxis_app_gateway_protocol::ModelUpgradeInfo;
use praxis_app_gateway_protocol::ReasoningEffortOption;
use praxis_app_gateway_protocol::RequestId;
use praxis_protocol::openai_models::ModelPreset;
use praxis_protocol::openai_models::known_openai_compatible_model_info;
use pretty_assertions::assert_eq;
use std::fs;
use tempfile::TempDir;
use tokio::time::timeout;

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(10);
const INVALID_REQUEST_ERROR_CODE: i64 = -32600;

fn model_from_preset(preset: &ModelPreset) -> Model {
    let model_info = known_openai_compatible_model_info(preset.model.as_str());
    Model {
        id: format!("openai::{}", preset.model),
        model_provider: Some("openai".to_string()),
        model: preset.model.clone(),
        upgrade: preset.upgrade.as_ref().map(|upgrade| upgrade.id.clone()),
        upgrade_info: preset.upgrade.as_ref().map(|upgrade| ModelUpgradeInfo {
            model: upgrade.id.clone(),
            upgrade_copy: upgrade.upgrade_copy.clone(),
            model_link: upgrade.model_link.clone(),
            migration_markdown: upgrade.migration_markdown.clone(),
        }),
        availability_nux: preset.availability_nux.clone().map(Into::into),
        display_name: preset.display_name.clone(),
        description: preset.description.clone(),
        hidden: !preset.show_in_picker,
        supported_reasoning_efforts: preset
            .supported_reasoning_efforts
            .iter()
            .map(|preset| ReasoningEffortOption {
                reasoning_effort: preset.effort,
                description: preset.description.clone(),
            })
            .collect(),
        default_reasoning_effort: preset.default_reasoning_effort,
        input_modalities: preset.input_modalities.clone(),
        // `write_models_cache()` round-trips through a simplified ModelInfo fixture that does not
        // preserve personality placeholders in base instructions, so app-gateway list results from
        // cache report `supports_personality = false`.
        supports_personality: false,
        supports_tools: true,
        supports_streaming: true,
        supports_parallel_tool_calls: model_info
            .as_ref()
            .is_some_and(|info| info.supports_parallel_tool_calls),
        context_window: Some(272_000),
        is_default: preset.is_default,
    }
}

fn expected_visible_models() -> Vec<Model> {
    // Filter by supported_in_api to support testing with both ChatGPT and non-ChatGPT auth modes.
    let mut presets = ModelPreset::filter_by_auth(
        praxis_core::test_support::all_model_presets().clone(),
        /*chatgpt_mode*/ false,
    );

    // Mirror `ModelsManager::build_available_models()` default selection after auth filtering.
    ModelPreset::mark_default_by_picker_visibility(&mut presets);

    presets
        .iter()
        .filter(|preset| preset.show_in_picker)
        .map(model_from_preset)
        .collect()
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

    let expected_models = expected_visible_models();

    assert_eq!(items, expected_models);
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

    let expected_models = expected_visible_models();
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
    assert!(next_cursor.is_none());
    Ok(())
}
