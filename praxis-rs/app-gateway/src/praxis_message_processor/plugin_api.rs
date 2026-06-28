use std::collections::HashMap;
use std::path::Path;
use std::path::PathBuf;

use praxis_app_gateway_protocol::MarketplaceInterface;
use praxis_app_gateway_protocol::MarketplaceLoadErrorInfo;
use praxis_app_gateway_protocol::PluginActivationDelta as ApiPluginActivationDelta;
use praxis_app_gateway_protocol::PluginAuthPolicy;
use praxis_app_gateway_protocol::PluginDetail;
use praxis_app_gateway_protocol::PluginDiagnostic;
use praxis_app_gateway_protocol::PluginDiagnosticSeverity;
use praxis_app_gateway_protocol::PluginInstallParams;
use praxis_app_gateway_protocol::PluginInstallPolicy;
use praxis_app_gateway_protocol::PluginInstallResponse;
use praxis_app_gateway_protocol::PluginInterface;
use praxis_app_gateway_protocol::PluginListParams;
use praxis_app_gateway_protocol::PluginListResponse;
use praxis_app_gateway_protocol::PluginMarketplaceEntry;
use praxis_app_gateway_protocol::PluginReadParams;
use praxis_app_gateway_protocol::PluginReadResponse;
use praxis_app_gateway_protocol::PluginSetEnabledParams;
use praxis_app_gateway_protocol::PluginSetEnabledResponse;
use praxis_app_gateway_protocol::PluginSource;
use praxis_app_gateway_protocol::PluginSummary;
use praxis_app_gateway_protocol::PluginSyncParams;
use praxis_app_gateway_protocol::PluginSyncResponse;
use praxis_app_gateway_protocol::PluginUninstallParams;
use praxis_app_gateway_protocol::PluginUninstallResponse;
use praxis_app_gateway_protocol::SkillInterface;
use praxis_app_gateway_protocol::SkillSummary;
use praxis_chatgpt::connectors;
use praxis_config::types::McpServerConfig;
use praxis_core::plugins::MarketplaceError;
use praxis_core::plugins::MarketplacePluginAuthPolicy;
use praxis_core::plugins::MarketplacePluginInstallPolicy;
use praxis_core::plugins::MarketplacePluginSource;
use praxis_core::plugins::OPENAI_CURATED_MARKETPLACE_NAME;
use praxis_core::plugins::PluginActivationDelta as CorePluginActivationDelta;
use praxis_core::plugins::PluginDiagnosticSeverity as CorePluginDiagnosticSeverity;
use praxis_core::plugins::PluginInstallError as CorePluginInstallError;
use praxis_core::plugins::PluginInstallRequest;
use praxis_core::plugins::PluginReadRequest;
use praxis_core::plugins::PluginSetEnabledError as CorePluginSetEnabledError;
use praxis_core::plugins::PluginUninstallError as CorePluginUninstallError;
use tracing::info;
use tracing::warn;

use super::PraxisMessageProcessor;
use super::plugin_app_helpers;
use crate::outgoing_message::ConnectionRequestId;

impl PraxisMessageProcessor {
    pub(super) async fn plugin_list(
        &self,
        request_id: ConnectionRequestId,
        params: PluginListParams,
    ) {
        let plugins_manager = self.thread_manager.plugins_manager();
        let PluginListParams {
            cwds,
            force_remote_sync,
        } = params;
        let roots = cwds.unwrap_or_default();

        let mut config = match self.load_latest_config(/*fallback_cwd*/ None).await {
            Ok(config) => config,
            Err(err) => {
                self.outgoing.send_error(request_id, err).await;
                return;
            }
        };
        let mut remote_sync_error = None;
        let auth = self.auth_manager.auth().await;

        if force_remote_sync {
            match plugins_manager
                .sync_plugins_from_remote(&config, auth.as_ref(), /*additive_only*/ false)
                .await
            {
                Ok(sync_result) => {
                    info!(
                        installed_plugin_ids = ?sync_result.installed_plugin_ids,
                        enabled_plugin_ids = ?sync_result.enabled_plugin_ids,
                        disabled_plugin_ids = ?sync_result.disabled_plugin_ids,
                        uninstalled_plugin_ids = ?sync_result.uninstalled_plugin_ids,
                        "completed plugin/catalog/list remote sync"
                    );
                }
                Err(err) => {
                    warn!(
                        error = %err,
                        "plugin/catalog/list remote sync failed; returning local marketplace state"
                    );
                    remote_sync_error = Some(err.to_string());
                }
            }

            config = match self.load_latest_config(/*fallback_cwd*/ None).await {
                Ok(config) => config,
                Err(err) => {
                    self.outgoing.send_error(request_id, err).await;
                    return;
                }
            };
        }

        let config_for_marketplace_listing = config.clone();
        let plugins_manager_for_marketplace_listing = plugins_manager.clone();
        let (data, marketplace_load_errors) = match tokio::task::spawn_blocking(move || {
            let outcome = plugins_manager_for_marketplace_listing
                .list_marketplaces_for_config(&config_for_marketplace_listing, &roots)?;
            Ok::<(Vec<PluginMarketplaceEntry>, Vec<MarketplaceLoadErrorInfo>), MarketplaceError>((
                outcome
                    .marketplaces
                    .into_iter()
                    .map(|marketplace| PluginMarketplaceEntry {
                        name: marketplace.name,
                        path: marketplace.path,
                        interface: marketplace.interface.map(|interface| MarketplaceInterface {
                            display_name: interface.display_name,
                        }),
                        plugins: marketplace
                            .plugins
                            .into_iter()
                            .map(|plugin| PluginSummary {
                                id: plugin.id,
                                installed: plugin.installed,
                                enabled: plugin.enabled,
                                name: plugin.name,
                                source: marketplace_plugin_source_to_info(plugin.source),
                                install_policy: plugin_install_policy_to_api(
                                    plugin.policy.installation,
                                ),
                                auth_policy: plugin_auth_policy_to_api(
                                    plugin.policy.authentication,
                                ),
                                interface: plugin.interface.map(plugin_interface_to_info),
                            })
                            .collect(),
                    })
                    .collect(),
                outcome
                    .errors
                    .into_iter()
                    .map(|err| MarketplaceLoadErrorInfo {
                        marketplace_path: err.path,
                        message: err.message,
                    })
                    .collect(),
            ))
        })
        .await
        {
            Ok(Ok(outcome)) => outcome,
            Ok(Err(err)) => {
                self.send_marketplace_error(request_id, err, "list marketplace plugins")
                    .await;
                return;
            }
            Err(err) => {
                self.send_internal_error(
                    request_id,
                    format!("failed to list marketplace plugins: {err}"),
                )
                .await;
                return;
            }
        };

        let featured_plugin_ids = if data
            .iter()
            .any(|marketplace| marketplace.name == OPENAI_CURATED_MARKETPLACE_NAME)
        {
            match plugins_manager
                .featured_plugin_ids_for_config(&config, auth.as_ref())
                .await
            {
                Ok(featured_plugin_ids) => featured_plugin_ids,
                Err(err) => {
                    warn!(
                        error = %err,
                        "plugin/catalog/list featured plugin fetch failed; returning empty featured ids"
                    );
                    Vec::new()
                }
            }
        } else {
            Vec::new()
        };

        self.outgoing
            .send_response(
                request_id,
                PluginListResponse {
                    marketplaces: data,
                    marketplace_load_errors,
                    remote_sync_error,
                    featured_plugin_ids,
                },
            )
            .await;
    }

    pub(super) async fn plugin_read(
        &self,
        request_id: ConnectionRequestId,
        params: PluginReadParams,
    ) {
        let plugins_manager = self.thread_manager.plugins_manager();
        let PluginReadParams {
            marketplace_path,
            plugin_name,
        } = params;
        let config_cwd = marketplace_path.as_path().parent().map(Path::to_path_buf);

        let config = match self.load_latest_config(config_cwd).await {
            Ok(config) => config,
            Err(err) => {
                self.outgoing.send_error(request_id, err).await;
                return;
            }
        };

        let request = PluginReadRequest {
            plugin_name,
            marketplace_path,
        };
        let config_for_read = config.clone();
        let outcome = match tokio::task::spawn_blocking(move || {
            plugins_manager.read_plugin_for_config(&config_for_read, &request)
        })
        .await
        {
            Ok(Ok(outcome)) => outcome,
            Ok(Err(err)) => {
                self.send_marketplace_error(request_id, err, "read plugin details")
                    .await;
                return;
            }
            Err(err) => {
                self.send_internal_error(
                    request_id,
                    format!("failed to read plugin details: {err}"),
                )
                .await;
                return;
            }
        };
        let app_summaries =
            plugin_app_helpers::load_plugin_app_summaries(&config, &outcome.plugin.apps).await;
        let visible_skills = outcome
            .plugin
            .skills
            .iter()
            .filter(|skill| {
                skill.matches_product_restriction_for_product(
                    self.thread_manager
                        .session_source()
                        .restriction_product()
                        .as_ref(),
                )
            })
            .cloned()
            .collect::<Vec<_>>();
        let plugin = PluginDetail {
            marketplace_name: outcome.marketplace_name,
            marketplace_path: outcome.marketplace_path,
            summary: PluginSummary {
                id: outcome.plugin.id,
                name: outcome.plugin.name,
                source: marketplace_plugin_source_to_info(outcome.plugin.source),
                installed: outcome.plugin.installed,
                enabled: outcome.plugin.enabled,
                install_policy: plugin_install_policy_to_api(outcome.plugin.policy.installation),
                auth_policy: plugin_auth_policy_to_api(outcome.plugin.policy.authentication),
                interface: outcome.plugin.interface.map(plugin_interface_to_info),
            },
            description: outcome.plugin.description,
            skills: plugin_skills_to_info(&visible_skills, &outcome.plugin.disabled_skill_paths),
            apps: app_summaries,
            mcp_servers: outcome.plugin.mcp_server_names,
        };

        self.outgoing
            .send_response(request_id, PluginReadResponse { plugin })
            .await;
    }

    pub(super) async fn plugin_sync(
        &self,
        request_id: ConnectionRequestId,
        params: PluginSyncParams,
    ) {
        let PluginSyncParams {
            marketplace_name,
            force,
        } = params;
        let config = match self.load_latest_config(/*fallback_cwd*/ None).await {
            Ok(config) => config,
            Err(err) => {
                self.outgoing.send_error(request_id, err).await;
                return;
            }
        };
        let plugins_manager = self.thread_manager.plugins_manager();

        if let Some(marketplace_name) = marketplace_name.as_deref()
            && marketplace_name != OPENAI_CURATED_MARKETPLACE_NAME
        {
            match plugins_manager
                .sync_marketplace_provider(&config, marketplace_name.to_string())
                .await
            {
                Ok(_outcome) => {
                    self.clear_plugin_related_caches();
                    self.outgoing
                        .send_response(
                            request_id,
                            PluginSyncResponse {
                                installed_plugin_ids: Vec::new(),
                                enabled_plugin_ids: Vec::new(),
                                disabled_plugin_ids: Vec::new(),
                                uninstalled_plugin_ids: Vec::new(),
                                error: None,
                            },
                        )
                        .await;
                }
                Err(err) => {
                    self.outgoing
                        .send_response(
                            request_id,
                            PluginSyncResponse {
                                installed_plugin_ids: Vec::new(),
                                enabled_plugin_ids: Vec::new(),
                                disabled_plugin_ids: Vec::new(),
                                uninstalled_plugin_ids: Vec::new(),
                                error: Some(err.to_string()),
                            },
                        )
                        .await;
                }
            }
            return;
        }

        let auth = self.auth_manager.auth().await;
        match plugins_manager
            .sync_plugins_from_remote(&config, auth.as_ref(), /*additive_only*/ !force)
            .await
        {
            Ok(result) => {
                self.clear_plugin_related_caches();
                self.outgoing
                    .send_response(
                        request_id,
                        PluginSyncResponse {
                            installed_plugin_ids: result.installed_plugin_ids,
                            enabled_plugin_ids: result.enabled_plugin_ids,
                            disabled_plugin_ids: result.disabled_plugin_ids,
                            uninstalled_plugin_ids: result.uninstalled_plugin_ids,
                            error: None,
                        },
                    )
                    .await;
            }
            Err(err) => {
                self.outgoing
                    .send_response(
                        request_id,
                        PluginSyncResponse {
                            installed_plugin_ids: Vec::new(),
                            enabled_plugin_ids: Vec::new(),
                            disabled_plugin_ids: Vec::new(),
                            uninstalled_plugin_ids: Vec::new(),
                            error: Some(err.to_string()),
                        },
                    )
                    .await;
            }
        }
    }

    pub(super) async fn plugin_install(
        &self,
        request_id: ConnectionRequestId,
        params: PluginInstallParams,
    ) {
        let PluginInstallParams {
            marketplace_path,
            plugin_name,
            force_remote_sync,
        } = params;
        let config_cwd = marketplace_path.as_path().parent().map(Path::to_path_buf);

        let plugins_manager = self.thread_manager.plugins_manager();
        let request = PluginInstallRequest {
            plugin_name,
            marketplace_path,
        };

        let install_result = if force_remote_sync {
            let config = match self.load_latest_config(config_cwd.clone()).await {
                Ok(config) => config,
                Err(err) => {
                    self.outgoing.send_error(request_id, err).await;
                    return;
                }
            };
            let auth = self.auth_manager.auth().await;
            plugins_manager
                .install_plugin_with_remote_sync(&config, auth.as_ref(), request)
                .await
        } else {
            plugins_manager.install_plugin(request).await
        };

        match install_result {
            Ok(result) => {
                let config = match self.load_latest_config(config_cwd).await {
                    Ok(config) => config,
                    Err(err) => {
                        warn!(
                            "failed to reload config after plugin install, using current config: {err:?}"
                        );
                        self.config.as_ref().clone()
                    }
                };

                self.clear_plugin_related_caches();

                let plugin_mcp_servers: HashMap<String, McpServerConfig> = result
                    .activation_delta
                    .changes
                    .mcp_servers
                    .iter()
                    .cloned()
                    .collect();

                if result.activation_delta.changes.mcp_servers_changed {
                    if let Err(err) = self.queue_mcp_server_refresh_for_config(&config).await {
                        warn!(
                            plugin = result.plugin_id.as_key(),
                            "failed to queue MCP refresh after plugin install: {err:?}"
                        );
                    }
                    if !plugin_mcp_servers.is_empty() {
                        self.start_plugin_mcp_oauth_logins(&config, plugin_mcp_servers)
                            .await;
                    }
                }

                let plugin_apps = result.activation_delta.changes.app_connector_ids.clone();
                let apps_needing_auth = if plugin_apps.is_empty()
                    || !config.features.apps_enabled(Some(&self.auth_manager)).await
                {
                    Vec::new()
                } else {
                    let (all_connectors_result, accessible_connectors_result) = tokio::join!(
                        connectors::list_all_connectors_with_options(
                            &config,
                            /*force_refetch*/ true,
                        ),
                        connectors::list_accessible_connectors_from_mcp_tools_with_options_and_status(
                            &config,
                            /*force_refetch*/ true,
                        ),
                    );

                    let all_connectors = match all_connectors_result {
                        Ok(connectors) => connectors,
                        Err(err) => {
                            warn!(
                                plugin = result.plugin_id.as_key(),
                                "failed to load app metadata after plugin install: {err:#}"
                            );
                            connectors::list_cached_all_connectors(&config)
                                .await
                                .unwrap_or_default()
                        }
                    };
                    let all_connectors =
                        connectors::connectors_for_plugin_apps(all_connectors, &plugin_apps);
                    let (accessible_connectors, praxis_apps_ready) =
                        match accessible_connectors_result {
                            Ok(status) => (status.connectors, status.praxis_apps_ready),
                            Err(err) => {
                                warn!(
                                    plugin = result.plugin_id.as_key(),
                                    "failed to load accessible apps after plugin install: {err:#}"
                                );
                                (
                                    connectors::list_cached_accessible_connectors_from_mcp_tools(
                                        &config,
                                    )
                                    .await
                                    .unwrap_or_default(),
                                    false,
                                )
                            }
                        };
                    if !praxis_apps_ready {
                        warn!(
                            plugin = result.plugin_id.as_key(),
                            "praxis_apps MCP not ready after plugin install; skipping appsNeedingAuth check"
                        );
                    }

                    plugin_app_helpers::plugin_apps_needing_auth(
                        &all_connectors,
                        &accessible_connectors,
                        &plugin_apps,
                        praxis_apps_ready,
                    )
                };

                self.outgoing
                    .send_response(
                        request_id,
                        PluginInstallResponse {
                            auth_policy: plugin_auth_policy_to_api(result.auth_policy),
                            apps_needing_auth,
                            activation_delta: plugin_activation_delta_to_api(
                                &result.activation_delta,
                            ),
                        },
                    )
                    .await;
            }
            Err(err) => {
                if err.is_invalid_request() {
                    self.send_invalid_request_error(request_id, err.to_string())
                        .await;
                    return;
                }

                match err {
                    CorePluginInstallError::Marketplace(err) => {
                        self.send_marketplace_error(request_id, err, "install plugin")
                            .await;
                    }
                    CorePluginInstallError::Config(err) => {
                        self.send_internal_error(
                            request_id,
                            format!("failed to persist installed plugin config: {err}"),
                        )
                        .await;
                    }
                    CorePluginInstallError::Remote(err) => {
                        self.send_internal_error(
                            request_id,
                            format!("failed to enable remote plugin: {err}"),
                        )
                        .await;
                    }
                    CorePluginInstallError::Join(err) => {
                        self.send_internal_error(
                            request_id,
                            format!("failed to install plugin: {err}"),
                        )
                        .await;
                    }
                    CorePluginInstallError::Store(err) => {
                        self.send_internal_error(
                            request_id,
                            format!("failed to install plugin: {err}"),
                        )
                        .await;
                    }
                }
            }
        }
    }

    pub(super) async fn plugin_uninstall(
        &self,
        request_id: ConnectionRequestId,
        params: PluginUninstallParams,
    ) {
        let PluginUninstallParams {
            plugin_id,
            force_remote_sync,
        } = params;
        let plugins_manager = self.thread_manager.plugins_manager();

        let uninstall_result = if force_remote_sync {
            let config = match self.load_latest_config(/*fallback_cwd*/ None).await {
                Ok(config) => config,
                Err(err) => {
                    self.outgoing.send_error(request_id, err).await;
                    return;
                }
            };
            let auth = self.auth_manager.auth().await;
            plugins_manager
                .uninstall_plugin_with_remote_sync(&config, auth.as_ref(), plugin_id)
                .await
        } else {
            plugins_manager.uninstall_plugin(plugin_id).await
        };

        match uninstall_result {
            Ok(activation_delta) => {
                self.clear_plugin_related_caches();
                if activation_delta.changes.mcp_servers_changed {
                    match self.load_latest_config(/*fallback_cwd*/ None).await {
                        Ok(config) => {
                            if let Err(err) =
                                self.queue_mcp_server_refresh_for_config(&config).await
                            {
                                warn!(
                                    "failed to queue MCP refresh after plugin uninstall: {err:?}"
                                );
                            }
                        }
                        Err(err) => warn!(
                            "failed to reload config after plugin uninstall; skipping MCP refresh: {err:?}"
                        ),
                    }
                }
                self.outgoing
                    .send_response(
                        request_id,
                        PluginUninstallResponse {
                            activation_delta: plugin_activation_delta_to_api(&activation_delta),
                        },
                    )
                    .await;
            }
            Err(err) => {
                if err.is_invalid_request() {
                    self.send_invalid_request_error(request_id, err.to_string())
                        .await;
                    return;
                }

                match err {
                    CorePluginUninstallError::Config(err) => {
                        self.send_internal_error(
                            request_id,
                            format!("failed to clear plugin config: {err}"),
                        )
                        .await;
                    }
                    CorePluginUninstallError::Remote(err) => {
                        self.send_internal_error(
                            request_id,
                            format!("failed to uninstall remote plugin: {err}"),
                        )
                        .await;
                    }
                    CorePluginUninstallError::Join(err) => {
                        self.send_internal_error(
                            request_id,
                            format!("failed to uninstall plugin: {err}"),
                        )
                        .await;
                    }
                    CorePluginUninstallError::Store(err) => {
                        self.send_internal_error(
                            request_id,
                            format!("failed to uninstall plugin: {err}"),
                        )
                        .await;
                    }
                    CorePluginUninstallError::InvalidPluginId(_) => {
                        unreachable!("invalid plugin ids are handled above");
                    }
                }
            }
        }
    }

    pub(super) async fn plugin_set_enabled(
        &self,
        request_id: ConnectionRequestId,
        params: PluginSetEnabledParams,
    ) {
        let plugins_manager = self.thread_manager.plugins_manager();
        let PluginSetEnabledParams { plugin_id, enabled } = params;
        match plugins_manager.set_plugin_enabled(plugin_id, enabled).await {
            Ok(activation_delta) => {
                self.clear_plugin_related_caches();
                if activation_delta.changes.mcp_servers_changed {
                    match self.load_latest_config(/*fallback_cwd*/ None).await {
                        Ok(config) => {
                            if let Err(err) =
                                self.queue_mcp_server_refresh_for_config(&config).await
                            {
                                warn!(
                                    "failed to queue MCP refresh after plugin enablement change: {err:?}"
                                );
                            }
                        }
                        Err(err) => warn!(
                            "failed to reload config after plugin enablement change; skipping MCP refresh: {err:?}"
                        ),
                    }
                }
                self.outgoing
                    .send_response(
                        request_id,
                        PluginSetEnabledResponse {
                            activation_delta: plugin_activation_delta_to_api(&activation_delta),
                        },
                    )
                    .await;
            }
            Err(err) => {
                if err.is_invalid_request() {
                    self.send_invalid_request_error(request_id, err.to_string())
                        .await;
                    return;
                }
                match err {
                    CorePluginSetEnabledError::Config(err) => {
                        self.send_internal_error(
                            request_id,
                            format!("failed to persist plugin enablement: {err}"),
                        )
                        .await;
                    }
                    CorePluginSetEnabledError::InvalidPluginId(_)
                    | CorePluginSetEnabledError::NotInstalled(_) => {
                        unreachable!("invalid setEnabled requests are handled above");
                    }
                }
            }
        }
    }
}

fn plugin_activation_delta_to_api(
    delta: &CorePluginActivationDelta<McpServerConfig>,
) -> ApiPluginActivationDelta {
    ApiPluginActivationDelta {
        plugin_id: delta.plugin_id.as_ref().map(|plugin_id| plugin_id.as_key()),
        installed_path: delta.installed_path.clone(),
        skills_changed: delta.changes.skills_changed,
        mcp_servers_changed: delta.changes.mcp_servers_changed,
        apps_changed: delta.changes.apps_changed,
        skill_roots: delta.changes.skill_roots.clone(),
        mcp_servers: delta
            .changes
            .mcp_servers
            .iter()
            .map(|(name, _)| name.clone())
            .collect(),
        app_connector_ids: delta
            .changes
            .app_connector_ids
            .iter()
            .map(|connector_id| connector_id.0.clone())
            .collect(),
        diagnostics: delta
            .diagnostics
            .iter()
            .map(|diagnostic| PluginDiagnostic {
                severity: match diagnostic.severity {
                    CorePluginDiagnosticSeverity::Error => PluginDiagnosticSeverity::Error,
                    CorePluginDiagnosticSeverity::Warning => PluginDiagnosticSeverity::Warning,
                    CorePluginDiagnosticSeverity::Info => PluginDiagnosticSeverity::Info,
                },
                code: diagnostic.code.clone(),
                message: diagnostic.message.clone(),
            })
            .collect(),
    }
}

fn plugin_skills_to_info(
    skills: &[praxis_core::skills::SkillMetadata],
    disabled_skill_paths: &std::collections::HashSet<PathBuf>,
) -> Vec<SkillSummary> {
    skills
        .iter()
        .map(|skill| SkillSummary {
            name: skill.name.clone(),
            description: skill.description.clone(),
            short_description: skill.short_description.clone(),
            interface: skill.interface.clone().map(|interface| SkillInterface {
                display_name: interface.display_name,
                short_description: interface.short_description,
                icon_small: interface.icon_small,
                icon_large: interface.icon_large,
                brand_color: interface.brand_color,
                default_prompt: interface.default_prompt,
            }),
            path: skill.path_to_skills_md.clone(),
            enabled: !disabled_skill_paths.contains(&skill.path_to_skills_md),
        })
        .collect()
}

fn plugin_interface_to_info(
    interface: praxis_core::plugins::PluginManifestInterface,
) -> PluginInterface {
    PluginInterface {
        display_name: interface.display_name,
        short_description: interface.short_description,
        long_description: interface.long_description,
        developer_name: interface.developer_name,
        category: interface.category,
        capabilities: interface.capabilities,
        website_url: interface.website_url,
        privacy_policy_url: interface.privacy_policy_url,
        terms_of_service_url: interface.terms_of_service_url,
        default_prompt: interface.default_prompt,
        brand_color: interface.brand_color,
        composer_icon: interface.composer_icon,
        logo: interface.logo,
        screenshots: interface.screenshots,
    }
}

fn marketplace_plugin_source_to_info(source: MarketplacePluginSource) -> PluginSource {
    match source {
        MarketplacePluginSource::Local { path } => PluginSource::Local { path },
    }
}

fn plugin_install_policy_to_api(policy: MarketplacePluginInstallPolicy) -> PluginInstallPolicy {
    match policy {
        MarketplacePluginInstallPolicy::NotAvailable => PluginInstallPolicy::NotAvailable,
        MarketplacePluginInstallPolicy::Available => PluginInstallPolicy::Available,
        MarketplacePluginInstallPolicy::InstalledByDefault => {
            PluginInstallPolicy::InstalledByDefault
        }
    }
}

fn plugin_auth_policy_to_api(policy: MarketplacePluginAuthPolicy) -> PluginAuthPolicy {
    match policy {
        MarketplacePluginAuthPolicy::OnInstall => PluginAuthPolicy::OnInstall,
        MarketplacePluginAuthPolicy::OnUse => PluginAuthPolicy::OnUse,
    }
}
