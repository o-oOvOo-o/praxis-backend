use super::LoaderOverrides;
use super::load_config_layers_state;
use crate::config::ConfigBuilder;
use crate::config::ConfigOverrides;
use crate::config::ConfigToml;
use crate::config::ConstraintError;
use crate::config::ProjectConfig;
use crate::config_loader::CloudRequirementsLoadError;
use crate::config_loader::CloudRequirementsLoader;
use crate::config_loader::ConfigLayerEntry;
use crate::config_loader::ConfigLoadError;
use crate::config_loader::ConfigRequirements;
use crate::config_loader::ConfigRequirementsToml;
use crate::config_loader::ConfigRequirementsWithSources;
use crate::config_loader::RequirementSource;
use crate::config_loader::load_requirements_toml;
use crate::config_loader::version_for_toml;
use praxis_config::CONFIG_TOML_FILE;
use praxis_protocol::config_types::TrustLevel;
use praxis_protocol::config_types::WebSearchMode;
use praxis_protocol::protocol::AskForApproval;
#[cfg(target_os = "macos")]
use praxis_protocol::protocol::SandboxPolicy;
use praxis_utils_absolute_path::AbsolutePathBuf;
use pretty_assertions::assert_eq;
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::path::Path;
use tempfile::tempdir;
use toml::Value as TomlValue;

fn config_error_from_io(err: &std::io::Error) -> &super::ConfigError {
    err.get_ref()
        .and_then(|err| err.downcast_ref::<ConfigLoadError>())
        .map(ConfigLoadError::config_error)
        .expect("expected ConfigLoadError")
}

async fn make_config_for_test(
    praxis_home: &Path,
    project_path: &Path,
    trust_level: TrustLevel,
    project_root_markers: Option<Vec<String>>,
) -> std::io::Result<()> {
    tokio::fs::write(
        praxis_home.join(CONFIG_TOML_FILE),
        toml::to_string(&ConfigToml {
            projects: Some(HashMap::from([(
                project_path.to_string_lossy().to_string(),
                ProjectConfig {
                    trust_level: Some(trust_level),
                },
            )])),
            project_root_markers,
            ..Default::default()
        })
        .expect("serialize config"),
    )
    .await
}

mod cloud_requirements;
mod config_errors;
mod layer_merging;
mod managed_preferences;
mod project_layers;
mod project_markers;
mod requirements_exec_policy;
