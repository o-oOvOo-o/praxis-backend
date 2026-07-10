use crate::app_event::FeedbackCategory;
use crate::app_gateway_session::app_gateway_rate_limit_snapshots_to_core;
use color_eyre::eyre::Result;
use color_eyre::eyre::WrapErr;
use praxis_app_gateway_client::AppGatewayRequestHandle;
use praxis_app_gateway_protocol::ClientRequest;
use praxis_app_gateway_protocol::FeedbackUploadParams;
use praxis_app_gateway_protocol::FeedbackUploadResponse;
use praxis_app_gateway_protocol::GetAccountRateLimitsResponse;
use praxis_app_gateway_protocol::ListMcpServerStatusParams;
use praxis_app_gateway_protocol::ListMcpServerStatusResponse;
use praxis_app_gateway_protocol::McpServerStatus;
use praxis_app_gateway_protocol::PluginCommandExecuteParams;
use praxis_app_gateway_protocol::PluginCommandExecuteResponse;
use praxis_app_gateway_protocol::PluginInstallParams;
use praxis_app_gateway_protocol::PluginInstallResponse;
use praxis_app_gateway_protocol::PluginListParams;
use praxis_app_gateway_protocol::PluginListResponse;
use praxis_app_gateway_protocol::PluginReadParams;
use praxis_app_gateway_protocol::PluginReadResponse;
use praxis_app_gateway_protocol::PluginUninstallParams;
use praxis_app_gateway_protocol::PluginUninstallResponse;
use praxis_app_gateway_protocol::RequestId;
use praxis_protocol::ThreadId;
use praxis_protocol::protocol::RateLimitSnapshot;
use praxis_utils_absolute_path::AbsolutePathBuf;
use std::path::PathBuf;
use uuid::Uuid;

use crate::bottom_pane::PluginCommandInvocation;

pub(crate) async fn fetch_all_mcp_server_statuses(
    request_handle: AppGatewayRequestHandle,
) -> Result<Vec<McpServerStatus>> {
    let mut cursor = None;
    let mut statuses = Vec::new();

    loop {
        let request_id = RequestId::String(format!("mcp-inventory-{}", Uuid::new_v4()));
        let response: ListMcpServerStatusResponse = request_handle
            .request_typed(ClientRequest::McpServerStatusList {
                request_id,
                params: ListMcpServerStatusParams {
                    cursor: cursor.clone(),
                    limit: Some(100),
                },
            })
            .await
            .wrap_err("mcpServerStatus/list failed in TUI")?;
        statuses.extend(response.data);
        if let Some(next_cursor) = response.next_cursor {
            cursor = Some(next_cursor);
        } else {
            break;
        }
    }

    Ok(statuses)
}

pub(crate) async fn fetch_account_rate_limits(
    request_handle: AppGatewayRequestHandle,
) -> Result<Vec<RateLimitSnapshot>> {
    let request_id = RequestId::String(format!("account-rate-limits-{}", Uuid::new_v4()));
    let response: GetAccountRateLimitsResponse = request_handle
        .request_typed(ClientRequest::GetAccountRateLimits {
            request_id,
            params: None,
        })
        .await
        .wrap_err("account/rateLimits/read failed in TUI")?;

    Ok(app_gateway_rate_limit_snapshots_to_core(response))
}

pub(crate) async fn fetch_plugins_list(
    request_handle: AppGatewayRequestHandle,
    cwd: PathBuf,
) -> Result<PluginListResponse> {
    let cwd = AbsolutePathBuf::try_from(cwd).wrap_err("plugin list cwd must be absolute")?;
    let request_id = RequestId::String(format!("plugin-list-{}", Uuid::new_v4()));
    request_handle
        .request_typed(ClientRequest::PluginList {
            request_id,
            params: PluginListParams {
                cwds: Some(vec![cwd]),
                force_remote_sync: false,
            },
        })
        .await
        .wrap_err("plugin/catalog/list failed in TUI")
}

pub(crate) async fn fetch_plugin_detail(
    request_handle: AppGatewayRequestHandle,
    params: PluginReadParams,
) -> Result<PluginReadResponse> {
    let request_id = RequestId::String(format!("plugin-read-{}", Uuid::new_v4()));
    request_handle
        .request_typed(ClientRequest::PluginRead { request_id, params })
        .await
        .wrap_err("plugin/read failed in TUI")
}

pub(crate) async fn fetch_plugin_install(
    request_handle: AppGatewayRequestHandle,
    marketplace_path: AbsolutePathBuf,
    plugin_name: String,
) -> Result<PluginInstallResponse> {
    let request_id = RequestId::String(format!("plugin-install-{}", Uuid::new_v4()));
    request_handle
        .request_typed(ClientRequest::PluginInstall {
            request_id,
            params: PluginInstallParams {
                marketplace_path,
                plugin_name,
                force_remote_sync: false,
            },
        })
        .await
        .wrap_err("plugin/install failed in TUI")
}

pub(crate) async fn fetch_plugin_uninstall(
    request_handle: AppGatewayRequestHandle,
    plugin_id: String,
) -> Result<PluginUninstallResponse> {
    let request_id = RequestId::String(format!("plugin-uninstall-{}", Uuid::new_v4()));
    request_handle
        .request_typed(ClientRequest::PluginUninstall {
            request_id,
            params: PluginUninstallParams {
                plugin_id,
                force_remote_sync: false,
            },
        })
        .await
        .wrap_err("plugin/uninstall failed in TUI")
}

pub(crate) async fn fetch_plugin_command_execute(
    request_handle: AppGatewayRequestHandle,
    command: &PluginCommandInvocation,
) -> Result<PluginCommandExecuteResponse> {
    let request_id = RequestId::String(format!("plugin-command-{}", Uuid::new_v4()));
    let args = shlex::split(command.args.as_str()).unwrap_or_else(|| {
        if command.args.trim().is_empty() {
            Vec::new()
        } else {
            vec![command.args.trim().to_string()]
        }
    });
    request_handle
        .request_typed(ClientRequest::PluginCommandExecute {
            request_id,
            params: PluginCommandExecuteParams {
                plugin_id: command.plugin_id.clone(),
                command_name: command.name.clone(),
                args,
            },
        })
        .await
        .wrap_err("pluginCommand/execute failed in TUI")
}

pub(crate) fn build_feedback_upload_params(
    origin_thread_id: Option<ThreadId>,
    rollout_path: Option<PathBuf>,
    category: FeedbackCategory,
    reason: Option<String>,
    include_logs: bool,
) -> FeedbackUploadParams {
    let extra_log_files = if include_logs {
        rollout_path.map(|rollout_path| vec![rollout_path])
    } else {
        None
    };
    FeedbackUploadParams {
        classification: crate::bottom_pane::feedback_classification(category).to_string(),
        reason,
        thread_id: origin_thread_id.map(|thread_id| thread_id.to_string()),
        include_logs,
        extra_log_files,
    }
}

pub(crate) async fn fetch_feedback_upload(
    request_handle: AppGatewayRequestHandle,
    params: FeedbackUploadParams,
) -> Result<FeedbackUploadResponse> {
    let request_id = RequestId::String(format!("feedback-upload-{}", Uuid::new_v4()));
    request_handle
        .request_typed(ClientRequest::FeedbackUpload { request_id, params })
        .await
        .wrap_err("feedback/upload failed in TUI")
}
