use std::sync::Arc;

use praxis_app_gateway_protocol::JSONRPCErrorError;
use praxis_app_gateway_protocol::ListMcpServerStatusParams;
use praxis_app_gateway_protocol::ListMcpServerStatusResponse;
use praxis_app_gateway_protocol::McpServerOauthLoginCompletedNotification;
use praxis_app_gateway_protocol::McpServerOauthLoginParams;
use praxis_app_gateway_protocol::McpServerOauthLoginResponse;
use praxis_app_gateway_protocol::McpServerRefreshResponse;
use praxis_app_gateway_protocol::McpServerStatus;
use praxis_app_gateway_protocol::ServerNotification;
use praxis_config::types::McpServerTransportConfig;
use praxis_core::config::Config;
use praxis_login::OpenAiAccountAuth;
use praxis_mcp::mcp::auth::discover_supported_scopes;
use praxis_mcp::mcp::auth::resolve_oauth_scopes;
use praxis_mcp::mcp::collect_mcp_snapshot;
use praxis_mcp::mcp::group_tools_by_server;
use praxis_protocol::protocol::McpAuthStatus as CoreMcpAuthStatus;
use praxis_protocol::protocol::McpServerRefreshConfig;
use praxis_rmcp_client::perform_oauth_login_return_url;

use super::PraxisMessageProcessor;
use crate::json_rpc_error::internal_error;
use crate::json_rpc_error::invalid_request;
use crate::outgoing_message::ConnectionRequestId;
use crate::outgoing_message::OutgoingMessageSender;

impl PraxisMessageProcessor {
    pub(super) async fn mcp_server_refresh(
        &self,
        request_id: ConnectionRequestId,
        _params: Option<()>,
    ) {
        let config = match self.load_latest_config(/*fallback_cwd*/ None).await {
            Ok(config) => config,
            Err(error) => {
                self.outgoing.send_error(request_id, error).await;
                return;
            }
        };

        if let Err(error) = self.queue_mcp_server_refresh_for_config(&config).await {
            self.outgoing.send_error(request_id, error).await;
            return;
        }

        let response = McpServerRefreshResponse {};
        self.outgoing.send_response(request_id, response).await;
    }

    pub(super) async fn queue_mcp_server_refresh_for_config(
        &self,
        config: &Config,
    ) -> Result<(), JSONRPCErrorError> {
        let configured_servers = self.thread_manager.mcp_manager().configured_servers(config);
        let mcp_servers = match serde_json::to_value(configured_servers) {
            Ok(value) => value,
            Err(err) => {
                return Err(internal_error(format!(
                    "failed to serialize MCP servers: {err}"
                )));
            }
        };

        let mcp_oauth_credentials_store_mode =
            match serde_json::to_value(config.mcp_oauth_credentials_store_mode) {
                Ok(value) => value,
                Err(err) => {
                    return Err(internal_error(format!(
                        "failed to serialize MCP OAuth credentials store mode: {err}"
                    )));
                }
            };

        let refresh_config = McpServerRefreshConfig {
            mcp_servers,
            mcp_oauth_credentials_store_mode,
        };

        let thread_manager = Arc::clone(&self.thread_manager);
        thread_manager.refresh_mcp_servers(refresh_config).await;
        Ok(())
    }

    pub(super) async fn mcp_server_oauth_login(
        &self,
        request_id: ConnectionRequestId,
        params: McpServerOauthLoginParams,
    ) {
        let config = match self.load_latest_config(/*fallback_cwd*/ None).await {
            Ok(config) => config,
            Err(error) => {
                self.outgoing.send_error(request_id, error).await;
                return;
            }
        };

        let McpServerOauthLoginParams {
            name,
            scopes,
            timeout_secs,
        } = params;

        let configured_servers = self
            .thread_manager
            .mcp_manager()
            .configured_servers(&config);
        let Some(server) = configured_servers.get(&name) else {
            let error = invalid_request(format!("No MCP server named '{name}' found."));
            self.outgoing.send_error(request_id, error).await;
            return;
        };

        let (url, http_headers, env_http_headers) = match &server.transport {
            McpServerTransportConfig::StreamableHttp {
                url,
                http_headers,
                env_http_headers,
                ..
            } => (url.clone(), http_headers.clone(), env_http_headers.clone()),
            _ => {
                let error =
                    invalid_request("OAuth login is only supported for streamable HTTP servers.");
                self.outgoing.send_error(request_id, error).await;
                return;
            }
        };

        let discovered_scopes = if scopes.is_none() && server.scopes.is_none() {
            discover_supported_scopes(&server.transport).await
        } else {
            None
        };
        let resolved_scopes =
            resolve_oauth_scopes(scopes, server.scopes.clone(), discovered_scopes);

        match perform_oauth_login_return_url(
            &name,
            &url,
            config.mcp_oauth_credentials_store_mode,
            http_headers,
            env_http_headers,
            &resolved_scopes.scopes,
            server.oauth_resource.as_deref(),
            timeout_secs,
            config.mcp_oauth_callback_port,
            config.mcp_oauth_callback_url.as_deref(),
        )
        .await
        {
            Ok(handle) => {
                let authorization_url = handle.authorization_url().to_string();
                let notification_name = name.clone();
                let outgoing = Arc::clone(&self.outgoing);

                tokio::spawn(async move {
                    let (success, error) = match handle.wait().await {
                        Ok(()) => (true, None),
                        Err(err) => (false, Some(err.to_string())),
                    };

                    let notification = ServerNotification::McpServerOauthLoginCompleted(
                        McpServerOauthLoginCompletedNotification {
                            name: notification_name,
                            success,
                            error,
                        },
                    );
                    outgoing.send_server_notification(notification).await;
                });

                let response = McpServerOauthLoginResponse { authorization_url };
                self.outgoing.send_response(request_id, response).await;
            }
            Err(err) => {
                let error =
                    internal_error(format!("failed to login to MCP server '{name}': {err}"));
                self.outgoing.send_error(request_id, error).await;
            }
        }
    }

    pub(super) async fn list_mcp_server_status(
        &self,
        request_id: ConnectionRequestId,
        params: ListMcpServerStatusParams,
    ) {
        let request = request_id.clone();

        let outgoing = Arc::clone(&self.outgoing);
        let config = match self.load_latest_config(/*fallback_cwd*/ None).await {
            Ok(config) => config,
            Err(error) => {
                self.outgoing.send_error(request, error).await;
                return;
            }
        };
        let mcp_config = config.to_mcp_config(self.thread_manager.plugins_manager().as_ref());
        let auth = self.auth_manager.auth().await;

        tokio::spawn(async move {
            Self::list_mcp_server_status_task(outgoing, request, params, config, mcp_config, auth)
                .await;
        });
    }

    async fn list_mcp_server_status_task(
        outgoing: Arc<OutgoingMessageSender>,
        request_id: ConnectionRequestId,
        params: ListMcpServerStatusParams,
        config: Config,
        mcp_config: praxis_mcp::mcp::McpConfig,
        auth: Option<OpenAiAccountAuth>,
    ) {
        let snapshot = collect_mcp_snapshot(
            &mcp_config,
            auth.as_ref(),
            request_id.request_id.to_string(),
        )
        .await;

        let tools_by_server = group_tools_by_server(&snapshot.tools);

        let mut server_names: Vec<String> = config
            .mcp_servers
            .keys()
            .cloned()
            .chain(snapshot.auth_statuses.keys().cloned())
            .chain(snapshot.resources.keys().cloned())
            .chain(snapshot.resource_templates.keys().cloned())
            .collect();
        server_names.sort();
        server_names.dedup();

        let total = server_names.len();
        let limit = params.limit.unwrap_or(total as u32).max(1) as usize;
        let effective_limit = limit.min(total);
        let start = match params.cursor {
            Some(cursor) => match cursor.parse::<usize>() {
                Ok(idx) => idx,
                Err(_) => {
                    let error = invalid_request(format!("invalid cursor: {cursor}"));
                    outgoing.send_error(request_id, error).await;
                    return;
                }
            },
            None => 0,
        };

        if start > total {
            let error =
                invalid_request(format!("cursor {start} exceeds total MCP servers {total}"));
            outgoing.send_error(request_id, error).await;
            return;
        }

        let end = start.saturating_add(effective_limit).min(total);

        let data: Vec<McpServerStatus> = server_names[start..end]
            .iter()
            .map(|name| McpServerStatus {
                name: name.clone(),
                tools: tools_by_server.get(name).cloned().unwrap_or_default(),
                resources: snapshot.resources.get(name).cloned().unwrap_or_default(),
                resource_templates: snapshot
                    .resource_templates
                    .get(name)
                    .cloned()
                    .unwrap_or_default(),
                auth_status: snapshot
                    .auth_statuses
                    .get(name)
                    .cloned()
                    .unwrap_or(CoreMcpAuthStatus::Unsupported)
                    .into(),
            })
            .collect();

        let next_cursor = if end < total {
            Some(end.to_string())
        } else {
            None
        };

        let response = ListMcpServerStatusResponse { data, next_cursor };

        outgoing.send_response(request_id, response).await;
    }
}
