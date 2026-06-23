use crate::config::edit::ConfigEdit;
use crate::config::edit::ConfigEditsBuilder;
use crate::config::edit::apply_blocking;
use crate::config_loader::RequirementSource;
use crate::plugins::PluginsManager;
use praxis_config::CONFIG_TOML_FILE;
use praxis_config::types::AppToolApproval;
use praxis_config::types::ApprovalsReviewer;
use praxis_config::types::BundledSkillsConfig;
use praxis_config::types::FeedbackConfigToml;
use praxis_config::types::HistoryPersistence;
use praxis_config::types::McpServerToolConfig;
use praxis_config::types::McpServerTransportConfig;
use praxis_config::types::MemoriesConfig;
use praxis_config::types::MemoriesToml;
use praxis_config::types::ToolSuggestDiscoverableType;
use praxis_features::Feature;
use praxis_features::FeaturesToml;
use praxis_protocol::permissions::FileSystemAccessMode;
use praxis_protocol::permissions::FileSystemPath;
use praxis_protocol::permissions::FileSystemSandboxEntry;
use praxis_protocol::permissions::FileSystemSandboxPolicy;
use praxis_protocol::permissions::FileSystemSpecialPath;
use praxis_protocol::permissions::NetworkSandboxPolicy;
use tempfile::tempdir;

use super::*;
use core_test_support::PathBufExt;
use core_test_support::PathExt;
use core_test_support::TempDirExt;
use core_test_support::test_absolute_path;
use pretty_assertions::assert_eq;

use std::collections::BTreeMap;
use std::collections::HashMap;

use std::path::Path;
use std::time::Duration;
use tempfile::TempDir;

fn stdio_mcp(command: &str) -> McpServerConfig {
    McpServerConfig {
        transport: McpServerTransportConfig::Stdio {
            command: command.to_string(),
            args: Vec::new(),
            env: None,
            env_vars: Vec::new(),
            cwd: None,
        },
        enabled: true,
        required: false,
        disabled_reason: None,
        startup_timeout_sec: None,
        tool_timeout_sec: None,
        enabled_tools: None,
        disabled_tools: None,
        scopes: None,
        oauth_resource: None,
        tools: HashMap::new(),
    }
}

fn http_mcp(url: &str) -> McpServerConfig {
    McpServerConfig {
        transport: McpServerTransportConfig::StreamableHttp {
            url: url.to_string(),
            bearer_token_env_var: None,
            http_headers: None,
            env_http_headers: None,
        },
        enabled: true,
        required: false,
        disabled_reason: None,
        startup_timeout_sec: None,
        tool_timeout_sec: None,
        enabled_tools: None,
        disabled_tools: None,
        scopes: None,
        oauth_resource: None,
        tools: HashMap::new(),
    }
}

struct PrecedenceTestFixture {
    cwd: TempDir,
    praxis_home: TempDir,
    cfg: ConfigToml,
    model_provider_map: HashMap<String, ModelProviderInfo>,
    openai_provider: ModelProviderInfo,
    openai_custom_provider: ModelProviderInfo,
}

impl PrecedenceTestFixture {
    fn cwd(&self) -> AbsolutePathBuf {
        self.cwd.abs()
    }

    fn cwd_path(&self) -> PathBuf {
        self.cwd.path().to_path_buf()
    }

    fn praxis_home(&self) -> PathBuf {
        self.praxis_home.path().to_path_buf()
    }
}

fn create_test_fixture() -> std::io::Result<PrecedenceTestFixture> {
    let toml = r#"
model = "o3"
approval_policy = "untrusted"

# Can be used to determine which profile to use if not specified by
# `ConfigOverrides`.
profile = "gpt3"

[analytics]
enabled = true

[model_providers.openai-custom]
name = "OpenAI custom"
base_url = "https://api.openai.com/v1"
env_key = "OPENAI_API_KEY"
wire_api = "responses"
request_max_retries = 4            # retry failed HTTP requests
stream_max_retries = 10            # retry dropped SSE streams
stream_idle_timeout_ms = 300000    # 5m idle timeout
websocket_connect_timeout_ms = 15000

[profiles.o3]
model = "o3"
model_provider = "openai"
approval_policy = "never"
model_reasoning_effort = "high"
model_reasoning_summary = "detailed"

[profiles.gpt3]
model = "gpt-3.5-turbo"
model_provider = "openai-custom"

[profiles.zdr]
model = "o3"
model_provider = "openai"
approval_policy = "on-failure"

[profiles.zdr.analytics]
enabled = false

[profiles.gpt5]
model = "gpt-5.1"
model_provider = "openai"
approval_policy = "on-failure"
model_reasoning_effort = "high"
model_reasoning_summary = "detailed"
model_verbosity = "high"
"#;

    let cfg: ConfigToml = toml::from_str(toml).expect("TOML deserialization should succeed");

    // Use a temporary directory for the cwd so it does not contain an
    // AGENTS.md file.
    let cwd_temp_dir = TempDir::new().unwrap();
    let cwd = cwd_temp_dir.path().to_path_buf();
    // Make it look like a Git repo so it does not search for AGENTS.md in
    // a parent folder, either.
    std::fs::write(cwd.join(".git"), "gitdir: nowhere")?;

    let praxis_home_temp_dir = TempDir::new().unwrap();

    let openai_custom_provider = ModelProviderInfo {
        name: "OpenAI custom".to_string(),
        base_url: Some("https://api.openai.com/v1".to_string()),
        env_key: Some("OPENAI_API_KEY".to_string()),
        wire_api: crate::WireApi::Responses,
        compat: None,
        env_key_instructions: None,
        experimental_bearer_token: None,
        auth: None,
        query_params: None,
        http_headers: None,
        env_http_headers: None,
        request_max_retries: Some(4),
        stream_max_retries: Some(10),
        stream_idle_timeout_ms: Some(300_000),
        websocket_connect_timeout_ms: Some(15_000),
        requires_openai_auth: false,
        supports_websockets: false,
    };
    let model_provider_map = {
        let mut model_provider_map =
            built_in_model_providers(/* openai_base_url */ /*openai_base_url*/ None);
        model_provider_map.insert("openai-custom".to_string(), openai_custom_provider.clone());
        model_provider_map
    };

    let openai_provider = model_provider_map
        .get("openai")
        .expect("openai provider should exist")
        .clone();

    Ok(PrecedenceTestFixture {
        cwd: cwd_temp_dir,
        praxis_home: praxis_home_temp_dir,
        cfg,
        model_provider_map,
        openai_provider,
        openai_custom_provider,
    })
}

mod base_toml;
mod compact_roles_catalog;
mod mcp_runtime;
mod permissions;
mod precedence;
mod realtime;
mod requirements_async;
mod sandbox_workspace_features;
mod trust_provider_project;
