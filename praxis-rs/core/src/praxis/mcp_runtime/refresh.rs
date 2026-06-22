use std::collections::HashMap;

use praxis_config::types::McpServerConfig;
use praxis_mcp::mcp::auth::compute_auth_statuses;
use praxis_mcp::mcp::with_praxis_apps_mcp;
use praxis_mcp::mcp_connection_manager::McpConnectionManager;
use praxis_mcp::mcp_connection_manager::SandboxState;
use praxis_mcp::mcp_connection_manager::praxis_apps_tools_cache_key;
use praxis_protocol::protocol::McpServerRefreshConfig;
use praxis_rmcp_client::OAuthCredentialsStoreMode;
use tokio_util::sync::CancellationToken;
use tracing::warn;

use crate::praxis::Session;
use crate::praxis::TurnContext;

impl Session {
    pub(super) async fn refresh_mcp_servers_inner(
        &self,
        turn_context: &TurnContext,
        mcp_servers: HashMap<String, McpServerConfig>,
        store_mode: OAuthCredentialsStoreMode,
    ) {
        let auth = self.services.auth_manager.auth().await;
        let config = self.get_config().await;
        let mcp_config = config.to_mcp_config(self.services.plugins_manager.as_ref());
        let tool_plugin_provenance = self
            .services
            .mcp_manager
            .tool_plugin_provenance(config.as_ref());
        let mcp_servers = with_praxis_apps_mcp(mcp_servers, auth.as_ref(), &mcp_config);
        let auth_statuses = compute_auth_statuses(mcp_servers.iter(), store_mode).await;
        let permissions = turn_context.effective_permissions();
        let sandbox_state = SandboxState {
            sandbox_policy: permissions.sandbox_policy.get().clone(),
            praxis_linux_sandbox_exe: turn_context.praxis_linux_sandbox_exe.clone(),
            sandbox_cwd: turn_context.cwd.to_path_buf(),
            use_legacy_landlock: turn_context.features.use_legacy_landlock(),
        };
        {
            let mut guard = self.services.mcp_startup_cancellation_token.lock().await;
            guard.cancel();
            *guard = CancellationToken::new();
        }
        let (refreshed_manager, cancel_token) = McpConnectionManager::new(
            &mcp_servers,
            store_mode,
            auth_statuses,
            &permissions.approval_policy,
            turn_context.sub_id.clone(),
            self.get_tx_event(),
            sandbox_state,
            config.praxis_home.clone(),
            praxis_apps_tools_cache_key(auth.as_ref()),
            tool_plugin_provenance,
        )
        .await;
        {
            let mut guard = self.services.mcp_startup_cancellation_token.lock().await;
            if guard.is_cancelled() {
                cancel_token.cancel();
            }
            *guard = cancel_token;
        }

        let mut manager = self.services.mcp_connection_manager.write().await;
        *manager = refreshed_manager;
    }

    pub(in crate::praxis) async fn refresh_mcp_servers_if_requested(
        &self,
        turn_context: &TurnContext,
    ) {
        let refresh_config = { self.pending_mcp_server_refresh_config.lock().await.take() };
        let Some(refresh_config) = refresh_config else {
            return;
        };

        let McpServerRefreshConfig {
            mcp_servers,
            mcp_oauth_credentials_store_mode,
        } = refresh_config;

        let mcp_servers =
            match serde_json::from_value::<HashMap<String, McpServerConfig>>(mcp_servers) {
                Ok(servers) => servers,
                Err(err) => {
                    warn!("failed to parse MCP server refresh config: {err}");
                    return;
                }
            };
        let store_mode = match serde_json::from_value::<OAuthCredentialsStoreMode>(
            mcp_oauth_credentials_store_mode,
        ) {
            Ok(mode) => mode,
            Err(err) => {
                warn!("failed to parse MCP OAuth refresh config: {err}");
                return;
            }
        };

        self.refresh_mcp_servers_inner(turn_context, mcp_servers, store_mode)
            .await;
    }

    pub(in crate::praxis) async fn queue_mcp_server_refresh(
        &self,
        refresh_config: McpServerRefreshConfig,
    ) {
        let mut guard = self.pending_mcp_server_refresh_config.lock().await;
        *guard = Some(refresh_config);
    }

    pub(crate) async fn refresh_mcp_servers_now(
        &self,
        turn_context: &TurnContext,
        mcp_servers: HashMap<String, McpServerConfig>,
        store_mode: OAuthCredentialsStoreMode,
    ) {
        self.refresh_mcp_servers_inner(turn_context, mcp_servers, store_mode)
            .await;
    }

    pub(in crate::praxis) async fn cancel_mcp_startup(&self) {
        self.services
            .mcp_startup_cancellation_token
            .lock()
            .await
            .cancel();
    }
}
