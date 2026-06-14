//! Validates that the collaboration mode list endpoint returns the expected default presets.
//!
//! The test drives the app gateway through the MCP harness and asserts that the list response
//! includes the plan and default modes with their default model and reasoning effort
//! settings, which keeps the API contract visible in one place.

#![allow(clippy::unwrap_used)]

use std::time::Duration;

use anyhow::Result;
use app_test_support::McpProcess;
use app_test_support::to_response;
use praxis_app_gateway_protocol::CollaborationModeListParams;
use praxis_app_gateway_protocol::CollaborationModeListResponse;
use praxis_app_gateway_protocol::CollaborationModeMask;
use praxis_app_gateway_protocol::JSONRPCResponse;
use praxis_app_gateway_protocol::RequestId;
use praxis_core::test_support::builtin_collaboration_mode_presets;
use pretty_assertions::assert_eq;
use tempfile::TempDir;
use tokio::time::timeout;

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(10);

/// Confirms the server returns the default collaboration mode presets in a stable order.
#[tokio::test]
async fn list_collaboration_modes_returns_presets() -> Result<()> {
    let praxis_home = TempDir::new()?;
    let mut mcp = McpProcess::new(praxis_home.path()).await?;

    timeout(DEFAULT_TIMEOUT, mcp.initialize()).await??;

    let request_id = mcp
        .send_list_collaboration_modes_request(CollaborationModeListParams::default())
        .await?;

    let response: JSONRPCResponse = timeout(
        DEFAULT_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(request_id)),
    )
    .await??;

    let CollaborationModeListResponse { data: items } =
        to_response::<CollaborationModeListResponse>(response)?;

    let expected: Vec<CollaborationModeMask> = builtin_collaboration_mode_presets()
        .into_iter()
        .map(|preset| CollaborationModeMask {
            name: preset.name,
            mode: preset.mode,
            model: preset.model,
            reasoning_effort: preset.reasoning_effort,
        })
        .collect();
    assert_eq!(expected, items);
    Ok(())
}
