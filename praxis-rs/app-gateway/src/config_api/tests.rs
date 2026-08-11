use super::*;
use praxis_analytics::AnalyticsEventsClient;
use praxis_app_gateway_protocol::NetworkDomainPermission;
use praxis_app_gateway_protocol::NetworkRequirements;
use praxis_app_gateway_protocol::NetworkUnixSocketPermission;
use praxis_app_gateway_protocol::SandboxMode;
use praxis_core::config_loader::CloudConfigBundle;
use praxis_core::config_loader::ConfigRequirementsToml;
use praxis_core::config_loader::NetworkDomainPermissionToml as CoreNetworkDomainPermissionToml;
use praxis_core::config_loader::NetworkDomainPermissionsToml as CoreNetworkDomainPermissionsToml;
use praxis_core::config_loader::NetworkRequirementsToml as CoreNetworkRequirementsToml;
use praxis_core::config_loader::NetworkUnixSocketPermissionToml as CoreNetworkUnixSocketPermissionToml;
use praxis_core::config_loader::NetworkUnixSocketPermissionsToml as CoreNetworkUnixSocketPermissionsToml;
use praxis_core::config_loader::ResidencyRequirement as CoreResidencyRequirement;
use praxis_core::config_loader::SandboxModeRequirement as CoreSandboxModeRequirement;
use praxis_features::Feature;
use praxis_login::AuthManager;
use praxis_login::OpenAiAccountAuth;
use praxis_protocol::config_types::WebSearchMode;
use praxis_protocol::protocol::AskForApproval as CoreAskForApproval;
use pretty_assertions::assert_eq;
use serde_json::json;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use tempfile::TempDir;

#[derive(Default)]
struct RecordingUserConfigReloader {
    call_count: AtomicUsize,
}

#[async_trait]
impl UserConfigReloader for RecordingUserConfigReloader {
    async fn reload_user_config(&self) {
        self.call_count.fetch_add(1, Ordering::Relaxed);
    }
}

#[test]
fn map_requirements_toml_to_api_converts_core_enums() {
    let requirements = ConfigRequirementsToml {
        allowed_approval_policies: Some(vec![
            CoreAskForApproval::Never,
            CoreAskForApproval::OnRequest,
        ]),
        allowed_sandbox_modes: Some(vec![
            CoreSandboxModeRequirement::ReadOnly,
            CoreSandboxModeRequirement::ExternalSandbox,
        ]),
        allowed_web_search_modes: Some(vec![
            praxis_core::config_loader::WebSearchModeRequirement::Cached,
        ]),
        guardian_developer_instructions: None,
        feature_requirements: Some(praxis_core::config_loader::FeatureRequirementsToml {
            entries: std::collections::BTreeMap::from([
                ("apps".to_string(), false),
                ("personality".to_string(), true),
            ]),
        }),
        mcp_servers: None,
        apps: None,
        rules: None,
        enforce_residency: Some(CoreResidencyRequirement::Us),
        network: Some(CoreNetworkRequirementsToml {
            enabled: Some(true),
            http_port: Some(8080),
            socks_port: Some(1080),
            allow_upstream_proxy: Some(false),
            dangerously_allow_non_loopback_proxy: Some(false),
            dangerously_allow_all_unix_sockets: Some(true),
            domains: Some(CoreNetworkDomainPermissionsToml {
                entries: std::collections::BTreeMap::from([
                    (
                        "api.openai.com".to_string(),
                        CoreNetworkDomainPermissionToml::Allow,
                    ),
                    (
                        "example.com".to_string(),
                        CoreNetworkDomainPermissionToml::Deny,
                    ),
                ]),
            }),
            managed_allowed_domains_only: Some(false),
            unix_sockets: Some(CoreNetworkUnixSocketPermissionsToml {
                entries: std::collections::BTreeMap::from([(
                    "/tmp/proxy.sock".to_string(),
                    CoreNetworkUnixSocketPermissionToml::Allow,
                )]),
            }),
            allow_local_binding: Some(true),
        }),
    };

    let mapped = map_requirements_toml_to_api(requirements);

    assert_eq!(
        mapped.allowed_approval_policies,
        Some(vec![
            praxis_app_gateway_protocol::AskForApproval::Never,
            praxis_app_gateway_protocol::AskForApproval::OnRequest,
        ])
    );
    assert_eq!(
        mapped.allowed_sandbox_modes,
        Some(vec![SandboxMode::ReadOnly]),
    );
    assert_eq!(
        mapped.allowed_web_search_modes,
        Some(vec![WebSearchMode::Cached, WebSearchMode::Disabled]),
    );
    assert_eq!(
        mapped.feature_requirements,
        Some(std::collections::BTreeMap::from([
            ("apps".to_string(), false),
            ("personality".to_string(), true),
        ])),
    );
    assert_eq!(
        mapped.enforce_residency,
        Some(praxis_app_gateway_protocol::ResidencyRequirement::Us),
    );
    assert_eq!(
        mapped.network,
        Some(NetworkRequirements {
            enabled: Some(true),
            http_port: Some(8080),
            socks_port: Some(1080),
            allow_upstream_proxy: Some(false),
            dangerously_allow_non_loopback_proxy: Some(false),
            dangerously_allow_all_unix_sockets: Some(true),
            domains: Some(std::collections::BTreeMap::from([
                ("api.openai.com".to_string(), NetworkDomainPermission::Allow,),
                ("example.com".to_string(), NetworkDomainPermission::Deny),
            ])),
            managed_allowed_domains_only: Some(false),
            unix_sockets: Some(std::collections::BTreeMap::from([(
                "/tmp/proxy.sock".to_string(),
                NetworkUnixSocketPermission::Allow,
            )])),
            allow_local_binding: Some(true),
        }),
    );
}

#[test]
fn map_requirements_toml_to_api_preserves_canonical_unix_socket_permissions() {
    let requirements = ConfigRequirementsToml {
        allowed_approval_policies: None,
        allowed_sandbox_modes: None,
        allowed_web_search_modes: None,
        guardian_developer_instructions: None,
        feature_requirements: None,
        mcp_servers: None,
        apps: None,
        rules: None,
        enforce_residency: None,
        network: Some(CoreNetworkRequirementsToml {
            enabled: None,
            http_port: None,
            socks_port: None,
            allow_upstream_proxy: None,
            dangerously_allow_non_loopback_proxy: None,
            dangerously_allow_all_unix_sockets: None,
            domains: None,
            managed_allowed_domains_only: None,
            unix_sockets: Some(CoreNetworkUnixSocketPermissionsToml {
                entries: std::collections::BTreeMap::from([(
                    "/tmp/ignored.sock".to_string(),
                    CoreNetworkUnixSocketPermissionToml::None,
                )]),
            }),
            allow_local_binding: None,
        }),
    };

    let mapped = map_requirements_toml_to_api(requirements);

    assert_eq!(
        mapped.network,
        Some(NetworkRequirements {
            enabled: None,
            http_port: None,
            socks_port: None,
            allow_upstream_proxy: None,
            dangerously_allow_non_loopback_proxy: None,
            dangerously_allow_all_unix_sockets: None,
            domains: None,
            managed_allowed_domains_only: None,
            unix_sockets: Some(std::collections::BTreeMap::from([(
                "/tmp/ignored.sock".to_string(),
                NetworkUnixSocketPermission::None,
            )])),
            allow_local_binding: None,
        }),
    );
}

#[test]
fn map_requirements_toml_to_api_normalizes_allowed_web_search_modes() {
    let requirements = ConfigRequirementsToml {
        allowed_approval_policies: None,
        allowed_sandbox_modes: None,
        allowed_web_search_modes: Some(Vec::new()),
        guardian_developer_instructions: None,
        feature_requirements: None,
        mcp_servers: None,
        apps: None,
        rules: None,
        enforce_residency: None,
        network: None,
    };

    let mapped = map_requirements_toml_to_api(requirements);

    assert_eq!(
        mapped.allowed_web_search_modes,
        Some(vec![WebSearchMode::Disabled])
    );
}

#[tokio::test]
async fn apply_runtime_feature_enablement_keeps_cli_overrides_above_config_and_runtime() {
    let praxis_home = TempDir::new().expect("create temp dir");
    std::fs::write(
        praxis_home.path().join("config.toml"),
        "[features]\napps = false\n",
    )
    .expect("write config");

    let mut config = praxis_core::config::ConfigBuilder::default()
        .praxis_home(praxis_home.path().to_path_buf())
        .fallback_cwd(Some(praxis_home.path().to_path_buf()))
        .cli_overrides(vec![(
            "features.apps".to_string(),
            TomlValue::Boolean(true),
        )])
        .build()
        .await
        .expect("load config");

    apply_runtime_feature_enablement(&mut config, &BTreeMap::from([("apps".to_string(), false)]));

    assert!(config.features.enabled(Feature::Apps));
}

#[tokio::test]
async fn apply_runtime_feature_enablement_keeps_cloud_pins_above_cli_and_runtime() {
    let praxis_home = TempDir::new().expect("create temp dir");

    let mut config = praxis_core::config::ConfigBuilder::default()
        .praxis_home(praxis_home.path().to_path_buf())
        .cli_overrides(vec![(
            "features.apps".to_string(),
            TomlValue::Boolean(true),
        )])
        .cloud_config_bundle(CloudConfigBundleLoader::new(async {
            Ok(Some(CloudConfigBundle::from_single_requirements(
                ConfigRequirementsToml {
                    feature_requirements: Some(
                        praxis_core::config_loader::FeatureRequirementsToml {
                            entries: BTreeMap::from([("apps".to_string(), false)]),
                        },
                    ),
                    ..Default::default()
                },
            )))
        }))
        .build()
        .await
        .expect("load config");

    apply_runtime_feature_enablement(&mut config, &BTreeMap::from([("apps".to_string(), true)]));

    assert!(!config.features.enabled(Feature::Apps));
}

#[tokio::test]
async fn batch_write_reloads_user_config_when_requested() {
    let praxis_home = TempDir::new().expect("create temp dir");
    let user_config_path = praxis_home.path().join("config.toml");
    std::fs::write(&user_config_path, "").expect("write config");
    let reloader = Arc::new(RecordingUserConfigReloader::default());
    let analytics_config = Arc::new(
        praxis_core::config::ConfigBuilder::default()
            .build()
            .await
            .expect("load analytics config"),
    );
    let auth_manager = AuthManager::from_auth_for_testing(OpenAiAccountAuth::from_api_key("test"));
    let config_api = ConfigApi::new(
        praxis_home.path().to_path_buf(),
        Arc::new(RwLock::new(Vec::new())),
        Arc::new(RwLock::new(BTreeMap::new())),
        LoaderOverrides::default(),
        Arc::new(RwLock::new(CloudConfigBundleLoader::default())),
        reloader.clone(),
        AnalyticsEventsClient::new(
            auth_manager,
            analytics_config
                .chatgpt_base_url
                .trim_end_matches('/')
                .to_string(),
            analytics_config.analytics_enabled,
        ),
    );

    let response = config_api
        .batch_write(ConfigBatchWriteParams {
            edits: vec![praxis_app_gateway_protocol::ConfigEdit {
                key_path: "model".to_string(),
                value: json!("gpt-5"),
                merge_strategy: praxis_app_gateway_protocol::MergeStrategy::Replace,
            }],
            file_path: Some(user_config_path.display().to_string()),
            expected_version: None,
            reload_user_config: true,
        })
        .await
        .expect("batch write should succeed");

    assert_eq!(
        response,
        ConfigWriteResponse {
            status: praxis_app_gateway_protocol::WriteStatus::Ok,
            version: response.version.clone(),
            file_path: praxis_utils_absolute_path::AbsolutePathBuf::try_from(
                user_config_path.clone()
            )
            .expect("absolute config path"),
            overridden_metadata: None,
        }
    );
    assert_eq!(
        std::fs::read_to_string(user_config_path).unwrap(),
        "model = \"gpt-5\"\n"
    );
    assert_eq!(reloader.call_count.load(Ordering::Relaxed), 1);
}
