use super::load_plugin::*;
use super::marketplace_provider::*;
use super::*;

impl PluginsManager {
    pub fn new(praxis_home: PathBuf) -> Self {
        Self::new_with_restriction_product(praxis_home, Some(Product::praxis()))
    }

    pub fn new_with_restriction_product(
        praxis_home: PathBuf,
        restriction_product: Option<Product>,
    ) -> Self {
        // Product restrictions are enforced at marketplace admission time for a given PRAXIS_HOME:
        // listing, install, and curated refresh all consult this restriction context before new
        // plugins enter local config or cache. After admission, runtime plugin loading trusts the
        // contents of that PRAXIS_HOME and does not re-filter configured plugins by product, so
        // already-admitted plugins may continue exposing MCP servers/tools from shared local state.
        //
        // This assumes a single PRAXIS_HOME is only used by one product.
        Self {
            praxis_home: praxis_home.clone(),
            store: PluginStore::new(praxis_home),
            featured_plugin_ids_cache: RwLock::new(None),
            cached_enabled_outcome: RwLock::new(None),
            remote_sync_lock: Mutex::new(()),
            restriction_product,
            analytics_events_client: RwLock::new(None),
        }
    }

    pub fn set_analytics_events_client(&self, analytics_events_client: AnalyticsEventsClient) {
        let mut stored_client = match self.analytics_events_client.write() {
            Ok(client_guard) => client_guard,
            Err(err) => err.into_inner(),
        };
        *stored_client = Some(analytics_events_client);
    }

    fn restriction_product_matches(&self, products: Option<&[Product]>) -> bool {
        match products {
            None => true,
            Some([]) => false,
            Some(products) => self
                .restriction_product
                .as_ref()
                .is_some_and(|product| product.matches_product_restriction(products)),
        }
    }

    pub fn plugins_for_config(&self, config: &Config) -> PluginLoadOutcome {
        self.plugins_for_config_with_force_reload(config, /*force_reload*/ false)
    }

    pub(crate) fn plugins_for_config_with_force_reload(
        &self,
        config: &Config,
        force_reload: bool,
    ) -> PluginLoadOutcome {
        if !config.features.enabled(Feature::Plugins) {
            return PluginLoadOutcome::default();
        }

        if !force_reload && let Some(outcome) = self.cached_enabled_outcome() {
            return outcome;
        }

        let outcome = load_plugins_from_layer_stack(
            &config.config_layer_stack,
            &self.store,
            self.restriction_product.clone(),
        );
        log_plugin_load_errors(&outcome);
        let mut cache = match self.cached_enabled_outcome.write() {
            Ok(cache) => cache,
            Err(err) => err.into_inner(),
        };
        *cache = Some(outcome.clone());
        outcome
    }

    pub fn clear_cache(&self) {
        let mut cached_enabled_outcome = match self.cached_enabled_outcome.write() {
            Ok(cache) => cache,
            Err(err) => err.into_inner(),
        };
        let mut featured_plugin_ids_cache = match self.featured_plugin_ids_cache.write() {
            Ok(cache) => cache,
            Err(err) => err.into_inner(),
        };
        *featured_plugin_ids_cache = None;
        *cached_enabled_outcome = None;
    }

    /// Resolve plugin skill roots for a config layer stack without touching the plugins cache.
    pub fn effective_skill_roots_for_layer_stack(
        &self,
        config_layer_stack: &ConfigLayerStack,
        plugins_feature_enabled: bool,
    ) -> Vec<PathBuf> {
        if !plugins_feature_enabled {
            return Vec::new();
        }
        load_plugins_from_layer_stack(
            config_layer_stack,
            &self.store,
            self.restriction_product.clone(),
        )
        .effective_skill_roots()
    }

    fn cached_enabled_outcome(&self) -> Option<PluginLoadOutcome> {
        match self.cached_enabled_outcome.read() {
            Ok(cache) => cache.clone(),
            Err(err) => err.into_inner().clone(),
        }
    }

    fn cached_featured_plugin_ids(
        &self,
        cache_key: &FeaturedPluginIdsCacheKey,
    ) -> Option<Vec<String>> {
        {
            let cache = match self.featured_plugin_ids_cache.read() {
                Ok(cache) => cache,
                Err(err) => err.into_inner(),
            };
            let now = Instant::now();
            if let Some(cached) = cache.as_ref()
                && now < cached.expires_at
                && cached.key == *cache_key
            {
                return Some(cached.featured_plugin_ids.clone());
            }
        }

        let mut cache = match self.featured_plugin_ids_cache.write() {
            Ok(cache) => cache,
            Err(err) => err.into_inner(),
        };
        let now = Instant::now();
        if cache
            .as_ref()
            .is_some_and(|cached| now >= cached.expires_at || cached.key != *cache_key)
        {
            *cache = None;
        }
        None
    }

    fn write_featured_plugin_ids_cache(
        &self,
        cache_key: FeaturedPluginIdsCacheKey,
        featured_plugin_ids: &[String],
    ) {
        let mut cache = match self.featured_plugin_ids_cache.write() {
            Ok(cache) => cache,
            Err(err) => err.into_inner(),
        };
        *cache = Some(CachedFeaturedPluginIds {
            key: cache_key,
            expires_at: Instant::now() + FEATURED_PLUGIN_IDS_CACHE_TTL,
            featured_plugin_ids: featured_plugin_ids.to_vec(),
        });
    }

    pub async fn featured_plugin_ids_for_config(
        &self,
        config: &Config,
        auth: Option<&OpenAiAccountAuth>,
    ) -> Result<Vec<String>, RemotePluginFetchError> {
        if !config.features.enabled(Feature::Plugins) {
            return Ok(Vec::new());
        }

        let cache_key = featured_plugin_ids_cache_key(config, auth);
        if let Some(featured_plugin_ids) = self.cached_featured_plugin_ids(&cache_key) {
            return Ok(featured_plugin_ids);
        }
        let featured_plugin_ids =
            fetch_remote_featured_plugin_ids(config, auth, self.restriction_product.clone())
                .await?;
        self.write_featured_plugin_ids_cache(cache_key, &featured_plugin_ids);
        Ok(featured_plugin_ids)
    }

    pub async fn install_plugin(
        &self,
        request: PluginInstallRequest,
    ) -> Result<PluginInstallOutcome, PluginInstallError> {
        let resolved = resolve_marketplace_plugin(
            &request.marketplace_path,
            &request.plugin_name,
            self.restriction_product.clone(),
        )?;
        self.install_resolved_plugin(resolved).await
    }

    pub async fn install_plugin_with_remote_sync(
        &self,
        config: &Config,
        auth: Option<&OpenAiAccountAuth>,
        request: PluginInstallRequest,
    ) -> Result<PluginInstallOutcome, PluginInstallError> {
        let resolved = resolve_marketplace_plugin(
            &request.marketplace_path,
            &request.plugin_name,
            self.restriction_product.clone(),
        )?;
        let plugin_id = resolved.plugin_id.as_key();
        // This only forwards the backend mutation before the local install flow. We rely on
        // `plugin/catalog/list(forceRemoteSync=true)` to sync local state rather than doing an extra
        // reconcile pass here.
        enable_remote_plugin(config, auth, &plugin_id)
            .await
            .map_err(PluginInstallError::from)?;
        self.install_resolved_plugin(resolved).await
    }

    async fn install_resolved_plugin(
        &self,
        resolved: ResolvedMarketplacePlugin,
    ) -> Result<PluginInstallOutcome, PluginInstallError> {
        let auth_policy = resolved.auth_policy;
        let plugin_version = if is_openai_curated_marketplace(&resolved.plugin_id.marketplace_name)
        {
            Some(
                read_curated_plugins_sha(self.praxis_home.as_path()).ok_or_else(|| {
                    PluginStoreError::Invalid(
                        "local curated marketplace sha is not available".to_string(),
                    )
                })?,
            )
        } else {
            None
        };
        let store = self.store.clone();
        let result: StorePluginInstallResult = tokio::task::spawn_blocking(move || {
            if let Some(plugin_version) = plugin_version {
                store.install_with_version(resolved.source_path, resolved.plugin_id, plugin_version)
            } else {
                store.install(resolved.source_path, resolved.plugin_id)
            }
        })
        .await
        .map_err(PluginInstallError::join)??;

        ConfigService::new_with_defaults(self.praxis_home.clone())
            .write_value(ConfigValueWriteParams {
                key_path: format!("plugins.{}", result.plugin_id.as_key()),
                value: json!({
                    "enabled": true,
                }),
                merge_strategy: MergeStrategy::Replace,
                file_path: None,
                expected_version: None,
            })
            .await
            .map(|_| ())
            .map_err(PluginInstallError::from)?;

        let analytics_events_client = match self.analytics_events_client.read() {
            Ok(client) => client.clone(),
            Err(err) => err.into_inner().clone(),
        };
        if let Some(analytics_events_client) = analytics_events_client {
            analytics_events_client.track_plugin_installed(plugin_telemetry_metadata_from_root(
                &result.plugin_id,
                result.installed_path.as_path(),
            ));
        }

        let activation_delta =
            plugin_activation_delta_from_root(&result.plugin_id, result.installed_path.as_path());

        Ok(PluginInstallOutcome {
            plugin_id: result.plugin_id,
            plugin_version: result.plugin_version,
            installed_path: result.installed_path,
            auth_policy,
            activation_delta,
        })
    }

    pub async fn uninstall_plugin(
        &self,
        plugin_id: String,
    ) -> Result<PluginActivationDelta<McpServerConfig>, PluginUninstallError> {
        let plugin_id = PluginId::parse(&plugin_id)?;
        self.uninstall_plugin_id(plugin_id).await
    }

    pub async fn uninstall_plugin_with_remote_sync(
        &self,
        config: &Config,
        auth: Option<&OpenAiAccountAuth>,
        plugin_id: String,
    ) -> Result<PluginActivationDelta<McpServerConfig>, PluginUninstallError> {
        let plugin_id = PluginId::parse(&plugin_id)?;
        let plugin_key = plugin_id.as_key();
        // This only forwards the backend mutation before the local uninstall flow. We rely on
        // `plugin/catalog/list(forceRemoteSync=true)` to sync local state rather than doing an extra
        // reconcile pass here.
        uninstall_remote_plugin(config, auth, &plugin_key)
            .await
            .map_err(PluginUninstallError::from)?;
        self.uninstall_plugin_id(plugin_id).await
    }

    pub async fn set_plugin_enabled(
        &self,
        plugin_id: String,
        enabled: bool,
    ) -> Result<PluginActivationDelta<McpServerConfig>, PluginSetEnabledError> {
        let plugin_id = PluginId::parse(&plugin_id)?;
        let plugin_root = self
            .store
            .active_plugin_root(&plugin_id)
            .ok_or_else(|| PluginSetEnabledError::NotInstalled(plugin_id.as_key()))?;
        let activation_delta = plugin_activation_delta_from_root(&plugin_id, plugin_root.as_path());

        ConfigService::new_with_defaults(self.praxis_home.clone())
            .write_value(ConfigValueWriteParams {
                key_path: format!("plugins.{}.enabled", plugin_id.as_key()),
                value: json!(enabled),
                merge_strategy: MergeStrategy::Replace,
                file_path: None,
                expected_version: None,
            })
            .await
            .map(|_| ())
            .map_err(PluginSetEnabledError::from)?;
        self.clear_cache();
        Ok(activation_delta)
    }

    async fn uninstall_plugin_id(
        &self,
        plugin_id: PluginId,
    ) -> Result<PluginActivationDelta<McpServerConfig>, PluginUninstallError> {
        let activation_delta = self
            .store
            .active_plugin_root(&plugin_id)
            .map(|plugin_root| plugin_activation_delta_from_root(&plugin_id, plugin_root.as_path()))
            .unwrap_or_else(|| PluginActivationDelta {
                plugin_id: Some(plugin_id.clone()),
                ..PluginActivationDelta::default()
            });
        let plugin_telemetry = self
            .store
            .active_plugin_root(&plugin_id)
            .map(|_| installed_plugin_telemetry_metadata(self.praxis_home.as_path(), &plugin_id));
        let store = self.store.clone();
        let plugin_id_for_store = plugin_id.clone();
        tokio::task::spawn_blocking(move || store.uninstall(&plugin_id_for_store))
            .await
            .map_err(PluginUninstallError::join)??;

        ConfigEditsBuilder::new(&self.praxis_home)
            .with_edits([ConfigEdit::ClearPath {
                segments: vec!["plugins".to_string(), plugin_id.as_key()],
            }])
            .apply()
            .await?;

        let analytics_events_client = match self.analytics_events_client.read() {
            Ok(client) => client.clone(),
            Err(err) => err.into_inner().clone(),
        };
        if let Some(plugin_telemetry) = plugin_telemetry
            && let Some(analytics_events_client) = analytics_events_client
        {
            analytics_events_client.track_plugin_uninstalled(plugin_telemetry);
        }

        Ok(activation_delta)
    }

    pub async fn sync_plugins_from_remote(
        &self,
        config: &Config,
        auth: Option<&OpenAiAccountAuth>,
        additive_only: bool,
    ) -> Result<RemotePluginSyncResult, PluginRemoteSyncError> {
        let _remote_sync_guard = self.remote_sync_lock.lock().await;

        if !config.features.enabled(Feature::Plugins) {
            return Ok(RemotePluginSyncResult::default());
        }

        info!("starting remote plugin sync");
        let remote_plugins = fetch_remote_plugin_status(config, auth)
            .await
            .map_err(PluginRemoteSyncError::from)?;
        let configured_plugins = configured_plugins_from_stack(&config.config_layer_stack);
        let curated_marketplace_path =
            AbsolutePathBuf::try_from(curated_plugins_marketplace_path(self.praxis_home.as_path()))
                .map_err(|_| PluginRemoteSyncError::LocalMarketplaceNotFound)?;
        let curated_marketplace = match load_marketplace(&curated_marketplace_path) {
            Ok(marketplace) => marketplace,
            Err(MarketplaceError::MarketplaceNotFound { .. }) => {
                return Err(PluginRemoteSyncError::LocalMarketplaceNotFound);
            }
            Err(err) => return Err(err.into()),
        };

        let marketplace_name = curated_marketplace.name.clone();
        let curated_plugin_version = read_curated_plugins_sha(self.praxis_home.as_path())
            .ok_or_else(|| {
                PluginStoreError::Invalid(
                    "local curated marketplace sha is not available".to_string(),
                )
            })?;
        let curated_plugins = unique_curated_marketplace_plugins(
            &marketplace_name,
            curated_marketplace.plugins,
            "remote sync",
        );
        let local_plugin_names = curated_plugins
            .iter()
            .map(|plugin| plugin.name.clone())
            .collect::<HashSet<_>>();
        let mut local_plugins = Vec::<(
            String,
            PluginId,
            AbsolutePathBuf,
            Option<bool>,
            Option<String>,
            bool,
        )>::new();
        for plugin in curated_plugins {
            let plugin_id = PluginId::new(plugin.name.clone(), marketplace_name.clone())?;
            let plugin_key = plugin_id.as_key();
            let current_enabled = configured_plugins
                .get(&plugin_key)
                .map(|plugin| plugin.enabled);
            let installed_version = self.store.active_plugin_version(&plugin_id);
            let product_allowed =
                self.restriction_product_matches(plugin.policy.products.as_deref());
            local_plugins.push((
                plugin.name,
                plugin_id,
                plugin.source_path,
                current_enabled,
                installed_version,
                product_allowed,
            ));
        }

        let mut remote_installed_plugin_names = HashSet::<String>::new();
        for plugin in remote_plugins {
            if plugin.marketplace_name != marketplace_name {
                return Err(PluginRemoteSyncError::UnknownRemoteMarketplace {
                    marketplace_name: plugin.marketplace_name,
                });
            }
            if !local_plugin_names.contains(&plugin.name) {
                warn!(
                    plugin = plugin.name,
                    marketplace = %marketplace_name,
                    "ignoring remote plugin missing from local marketplace during sync"
                );
                continue;
            }
            // For now, sync treats remote `enabled = false` as uninstall rather than a distinct
            // disabled state.
            // TODO: Switch sync to `plugins/installed` so install and enable states stay distinct.
            if !plugin.enabled {
                continue;
            }
            if !remote_installed_plugin_names.insert(plugin.name.clone()) {
                return Err(PluginRemoteSyncError::DuplicateRemotePlugin {
                    plugin_name: plugin.name,
                });
            }
        }

        let mut config_edits = Vec::new();
        let mut installs = Vec::new();
        let mut uninstalls = Vec::new();
        let mut result = RemotePluginSyncResult::default();
        let remote_plugin_count = remote_installed_plugin_names.len();
        let local_plugin_count = local_plugins.len();

        for (
            plugin_name,
            plugin_id,
            source_path,
            current_enabled,
            installed_version,
            product_allowed,
        ) in local_plugins
        {
            let plugin_key = plugin_id.as_key();
            let is_installed = installed_version.is_some();
            if !product_allowed {
                continue;
            }
            if remote_installed_plugin_names.contains(&plugin_name) {
                if !is_installed {
                    installs.push((
                        source_path,
                        plugin_id.clone(),
                        curated_plugin_version.clone(),
                    ));
                }
                if !is_installed {
                    result.installed_plugin_ids.push(plugin_key.clone());
                }

                if current_enabled != Some(true) {
                    result.enabled_plugin_ids.push(plugin_key.clone());
                    config_edits.push(ConfigEdit::SetPath {
                        segments: vec!["plugins".to_string(), plugin_key, "enabled".to_string()],
                        value: value(true),
                    });
                }
            } else if !additive_only {
                if is_installed {
                    uninstalls.push(plugin_id);
                }
                if is_installed || current_enabled.is_some() {
                    result.uninstalled_plugin_ids.push(plugin_key.clone());
                }
                if current_enabled.is_some() {
                    config_edits.push(ConfigEdit::ClearPath {
                        segments: vec!["plugins".to_string(), plugin_key],
                    });
                }
            }
        }

        let store = self.store.clone();
        let store_result = tokio::task::spawn_blocking(move || {
            for (source_path, plugin_id, plugin_version) in installs {
                store.install_with_version(source_path, plugin_id, plugin_version)?;
            }
            for plugin_id in uninstalls {
                store.uninstall(&plugin_id)?;
            }
            Ok::<(), PluginStoreError>(())
        })
        .await
        .map_err(PluginRemoteSyncError::join)?;
        if let Err(err) = store_result {
            self.clear_cache();
            return Err(err.into());
        }

        let config_result = if config_edits.is_empty() {
            Ok(())
        } else {
            ConfigEditsBuilder::new(&self.praxis_home)
                .with_edits(config_edits)
                .apply()
                .await
        };
        self.clear_cache();
        config_result?;

        info!(
            marketplace = %marketplace_name,
            remote_plugin_count,
            local_plugin_count,
            installed_plugin_ids = ?result.installed_plugin_ids,
            enabled_plugin_ids = ?result.enabled_plugin_ids,
            disabled_plugin_ids = ?result.disabled_plugin_ids,
            uninstalled_plugin_ids = ?result.uninstalled_plugin_ids,
            "completed remote plugin sync"
        );

        Ok(result)
    }

    pub async fn sync_marketplace_provider(
        &self,
        config: &Config,
        marketplace_name: String,
    ) -> Result<PluginMarketplaceSyncOutcome, PluginMarketplaceProviderSyncError> {
        let marketplace = config
            .plugin_marketplaces
            .get(&marketplace_name)
            .ok_or_else(|| {
                PluginMarketplaceProviderSyncError::NotConfigured(marketplace_name.clone())
            })?;
        if !marketplace.enabled {
            return Err(PluginMarketplaceProviderSyncError::Disabled(
                marketplace_name,
            ));
        }

        let outcome = match &marketplace.provider {
            PluginMarketplaceProviderConfig::Local { path } => PluginMarketplaceSyncOutcome {
                marketplace_name,
                changed: false,
                local_root: Some(path.as_path().to_path_buf()),
                version: None,
                diagnostics: Vec::new(),
            },
            PluginMarketplaceProviderConfig::Git {
                repo,
                reference,
                path,
            } => {
                if praxis_plugin::validate_plugin_segment(&marketplace_name, "marketplace name")
                    .is_err()
                {
                    return Err(PluginMarketplaceProviderSyncError::InvalidMarketplaceName(
                        marketplace_name,
                    ));
                }
                let praxis_home = self.praxis_home.clone();
                let marketplace_name_for_task = marketplace_name.clone();
                let repo = repo.clone();
                let reference = reference.clone();
                let path = path.clone();
                tokio::task::spawn_blocking(move || {
                    sync_git_marketplace_provider(
                        praxis_home.as_path(),
                        &marketplace_name_for_task,
                        &repo,
                        reference.as_deref(),
                        path.as_deref(),
                    )
                })
                .await
                .map_err(PluginMarketplaceProviderSyncError::from)?
                .map_err(|message| PluginMarketplaceProviderSyncError::Git {
                    marketplace_name,
                    message,
                })?
            }
            PluginMarketplaceProviderConfig::Http { .. } => {
                return Err(PluginMarketplaceProviderSyncError::UnsupportedProvider {
                    marketplace_name,
                    provider: "http",
                });
            }
        };

        self.clear_cache();
        Ok(outcome)
    }

    pub fn list_marketplaces_for_config(
        &self,
        config: &Config,
        additional_roots: &[AbsolutePathBuf],
    ) -> Result<ConfiguredMarketplaceListOutcome, MarketplaceError> {
        if !config.features.enabled(Feature::Plugins) {
            return Ok(ConfiguredMarketplaceListOutcome::default());
        }

        let (installed_plugins, enabled_plugins) = self.configured_plugin_states(config);
        let marketplace_outcome =
            list_marketplaces(&self.marketplace_roots(config, additional_roots))?;
        let mut seen_plugin_keys = HashSet::new();
        let marketplaces = marketplace_outcome
            .marketplaces
            .into_iter()
            .filter_map(|marketplace| {
                let marketplace_name = marketplace.name.clone();
                let plugins = marketplace
                    .plugins
                    .into_iter()
                    .filter_map(|plugin| {
                        let plugin_key = format!("{}@{marketplace_name}", plugin.name);
                        if !seen_plugin_keys.insert(plugin_key.clone()) {
                            return None;
                        }
                        if !self.restriction_product_matches(plugin.policy.products.as_deref()) {
                            return None;
                        }

                        Some(ConfiguredMarketplacePlugin {
                            // Enabled state is keyed by `<plugin>@<marketplace>`, so duplicate
                            // plugin entries from duplicate marketplace files intentionally
                            // resolve to the first discovered source.
                            id: plugin_key.clone(),
                            installed: installed_plugins.contains(&plugin_key),
                            enabled: enabled_plugins.contains(&plugin_key),
                            name: plugin.name,
                            source: plugin.source,
                            policy: plugin.policy,
                            interface: plugin.interface,
                            llm: plugin.llm,
                        })
                    })
                    .collect::<Vec<_>>();

                (!plugins.is_empty()).then_some(ConfiguredMarketplace {
                    name: marketplace.name,
                    path: marketplace.path,
                    interface: openai_curated_marketplace_interface(&marketplace_name)
                        .or(marketplace.interface),
                    plugins,
                })
            })
            .collect();

        Ok(ConfiguredMarketplaceListOutcome {
            marketplaces,
            errors: marketplace_outcome.errors,
        })
    }

    pub fn read_plugin_for_config(
        &self,
        config: &Config,
        request: &PluginReadRequest,
    ) -> Result<PluginReadOutcome, MarketplaceError> {
        if !config.features.enabled(Feature::Plugins) {
            return Err(MarketplaceError::PluginsDisabled);
        }

        let marketplace = load_marketplace(&request.marketplace_path)?;
        let marketplace_name = marketplace.name.clone();
        let plugin = marketplace
            .plugins
            .into_iter()
            .find(|plugin| plugin.name == request.plugin_name);
        let Some(plugin) = plugin else {
            return Err(MarketplaceError::PluginNotFound {
                plugin_name: request.plugin_name.clone(),
                marketplace_name,
            });
        };
        if !self.restriction_product_matches(plugin.policy.products.as_deref()) {
            return Err(MarketplaceError::PluginNotFound {
                plugin_name: request.plugin_name.clone(),
                marketplace_name,
            });
        }

        let plugin_id = PluginId::new(plugin.name.clone(), marketplace.name.clone()).map_err(
            |err| match err {
                PluginIdError::Invalid(message) => MarketplaceError::InvalidPlugin(message),
            },
        )?;
        let plugin_key = plugin_id.as_key();
        let (installed_plugins, enabled_plugins) = self.configured_plugin_states(config);
        let source_path = match &plugin.source {
            MarketplacePluginSource::Local { path } => path.clone(),
        };
        if !source_path.as_path().is_dir() {
            return Err(MarketplaceError::InvalidPlugin(
                "path does not exist or is not a directory".to_string(),
            ));
        }
        let manifest = load_plugin_manifest(source_path.as_path()).ok_or_else(|| {
            MarketplaceError::InvalidPlugin(
                "missing or invalid .praxis-plugin/plugin.json".to_string(),
            )
        })?;
        let description = manifest.description.clone();
        let manifest_paths = &manifest.paths;
        let skill_config_rules = skill_config_rules_from_stack(&config.config_layer_stack);
        let resolved_skills = load_plugin_skills(
            source_path.as_path(),
            manifest_paths,
            self.restriction_product.clone(),
            &skill_config_rules,
        );
        let apps = load_plugin_apps(source_path.as_path());
        let mcp_config_paths = plugin_mcp_config_paths(source_path.as_path(), manifest_paths);
        let mut mcp_server_names = Vec::new();
        for mcp_config_path in mcp_config_paths {
            mcp_server_names.extend(
                load_mcp_servers_from_file(source_path.as_path(), &mcp_config_path)
                    .mcp_servers
                    .into_keys(),
            );
        }
        mcp_server_names.sort_unstable();
        mcp_server_names.dedup();

        Ok(PluginReadOutcome {
            marketplace_name: openai_curated_marketplace_display_name(&marketplace.name)
                .map(str::to_string)
                .unwrap_or(marketplace.name),
            marketplace_path: marketplace.path,
            plugin: PluginDetail {
                id: plugin_key.clone(),
                name: plugin.name,
                description,
                source: plugin.source,
                policy: plugin.policy,
                interface: plugin.interface,
                llm: manifest.llm,
                installed: installed_plugins.contains(&plugin_key),
                enabled: enabled_plugins.contains(&plugin_key),
                skills: resolved_skills.skills,
                disabled_skill_paths: resolved_skills.disabled_skill_paths,
                apps,
                mcp_server_names,
            },
        })
    }

    pub fn maybe_start_plugin_startup_tasks_for_config(
        self: &Arc<Self>,
        config: &Config,
        auth_manager: Arc<AuthManager>,
    ) {
        if config.features.enabled(Feature::Plugins) {
            self.start_curated_repo_sync();
            start_startup_remote_plugin_sync_once(
                Arc::clone(self),
                self.praxis_home.clone(),
                config.clone(),
                auth_manager.clone(),
            );

            let config = config.clone();
            let manager = Arc::clone(self);
            spawn_logged_startup_task("startup-featured-plugin-cache-warm", async move {
                let auth = auth_manager.auth().await;
                if let Err(err) = manager
                    .featured_plugin_ids_for_config(&config, auth.as_ref())
                    .await
                {
                    warn!(
                        error = %err,
                        "failed to warm featured plugin ids cache"
                    );
                }
            });
        }
    }

    fn start_curated_repo_sync(self: &Arc<Self>) {
        if CURATED_REPO_SYNC_STARTED.swap(true, Ordering::SeqCst) {
            return;
        }
        let manager = Arc::clone(self);
        let praxis_home = self.praxis_home.clone();
        spawn_logged_startup_task("plugins-curated-repo-sync", async move {
            match sync_curated_plugins_repo(praxis_home.as_path()).await {
                Ok(curated_plugin_version) => {
                    let praxis_home_for_refresh = praxis_home.clone();
                    let refresh_result = tokio::task::spawn_blocking(move || {
                        let configured_curated_plugin_ids =
                            configured_curated_plugin_ids_from_praxis_home(
                                praxis_home_for_refresh.as_path(),
                            );
                        refresh_curated_plugin_cache(
                            praxis_home_for_refresh.as_path(),
                            &curated_plugin_version,
                            &configured_curated_plugin_ids,
                        )
                    })
                    .await;
                    match refresh_result {
                        Ok(Ok(cache_refreshed)) => {
                            if cache_refreshed {
                                manager.clear_cache();
                            }
                        }
                        Ok(Err(err)) => {
                            manager.clear_cache();
                            CURATED_REPO_SYNC_STARTED.store(false, Ordering::SeqCst);
                            warn!("failed to refresh curated plugin cache after sync: {err}");
                        }
                        Err(err) => {
                            manager.clear_cache();
                            CURATED_REPO_SYNC_STARTED.store(false, Ordering::SeqCst);
                            warn!("failed to join curated plugin cache refresh task: {err}");
                        }
                    }
                }
                Err(err) => {
                    CURATED_REPO_SYNC_STARTED.store(false, Ordering::SeqCst);
                    warn!("failed to sync curated plugins repo: {err}");
                }
            }
        });
    }

    fn configured_plugin_states(&self, config: &Config) -> (HashSet<String>, HashSet<String>) {
        let configured_plugins = configured_plugins_from_stack(&config.config_layer_stack);
        let installed_plugins = configured_plugins
            .keys()
            .filter(|plugin_key| {
                PluginId::parse(plugin_key)
                    .ok()
                    .is_some_and(|plugin_id| self.store.is_installed(&plugin_id))
            })
            .cloned()
            .collect::<HashSet<_>>();
        let enabled_plugins = configured_plugins
            .into_iter()
            .filter_map(|(plugin_key, plugin)| plugin.enabled.then_some(plugin_key))
            .collect::<HashSet<_>>();
        (installed_plugins, enabled_plugins)
    }

    fn marketplace_roots(
        &self,
        config: &Config,
        additional_roots: &[AbsolutePathBuf],
    ) -> Vec<AbsolutePathBuf> {
        let mut roots = additional_roots.to_vec();
        let curated_repo_root = curated_plugins_repo_path(self.praxis_home.as_path());
        if curated_repo_root.is_dir()
            && let Ok(curated_repo_root) = AbsolutePathBuf::try_from(curated_repo_root)
        {
            roots.push(curated_repo_root);
        }
        roots.extend(self.configured_marketplace_roots(config));
        roots.sort_unstable_by(|left, right| left.as_path().cmp(right.as_path()));
        roots.dedup();
        roots
    }

    fn configured_marketplace_roots(&self, config: &Config) -> Vec<AbsolutePathBuf> {
        let mut roots = Vec::new();
        for (name, marketplace) in &config.plugin_marketplaces {
            if !marketplace.enabled {
                continue;
            }

            match &marketplace.provider {
                PluginMarketplaceProviderConfig::Local { path } => roots.push(path.clone()),
                PluginMarketplaceProviderConfig::Git { path, .. } => {
                    if let Some(root) = self.cached_marketplace_root(name, path.as_deref()) {
                        roots.push(root);
                    }
                }
                PluginMarketplaceProviderConfig::Http { .. } => {
                    if let Some(root) = self.cached_marketplace_root(name, None) {
                        roots.push(root);
                    }
                }
            }
        }
        roots
    }

    fn cached_marketplace_root(
        &self,
        marketplace_name: &str,
        relative_path: Option<&Path>,
    ) -> Option<AbsolutePathBuf> {
        if praxis_plugin::validate_plugin_segment(marketplace_name, "marketplace name").is_err() {
            warn!(
                marketplace = marketplace_name,
                "ignoring invalid plugin marketplace cache name"
            );
            return None;
        }
        let mut root = self
            .praxis_home
            .join(MARKETPLACE_PROVIDER_CACHE_DIR)
            .join(marketplace_name);
        if let Some(relative_path) = relative_path {
            root.push(relative_path);
        }
        root.is_dir()
            .then(|| AbsolutePathBuf::try_from(root).ok())
            .flatten()
    }

    pub fn configured_marketplace_refs(&self, config: &Config) -> Vec<PluginMarketplaceRef> {
        let mut refs = config
            .plugin_marketplaces
            .iter()
            .map(|(name, marketplace)| PluginMarketplaceRef {
                name: name.clone(),
                display_name: marketplace.display_name.clone(),
                provider: marketplace_provider_source(&marketplace.provider),
                enabled: marketplace.enabled,
                sync_on_startup: marketplace.sync_on_startup,
            })
            .collect::<Vec<_>>();
        refs.sort_unstable_by(|left, right| left.name.cmp(&right.name));
        refs
    }
}
