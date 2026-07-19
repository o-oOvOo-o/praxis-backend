pub(super) use super::LoaderOverrides;
pub(super) use super::load_config_layers_state;
pub(super) use crate::config::ConfigBuilder;
pub(super) use crate::config::ConfigOverrides;
pub(super) use crate::config::ConfigToml;
pub(super) use crate::config::ConstraintError;
pub(super) use crate::config::ProjectConfig;
pub(super) use crate::config_loader::CloudRequirementsLoadError;
pub(super) use crate::config_loader::CloudRequirementsLoader;
pub(super) use crate::config_loader::ConfigLayerEntry;
pub(super) use crate::config_loader::ConfigLoadError;
pub(super) use crate::config_loader::ConfigRequirements;
pub(super) use crate::config_loader::ConfigRequirementsToml;
pub(super) use crate::config_loader::ConfigRequirementsWithSources;
pub(super) use crate::config_loader::RequirementSource;
pub(super) use crate::config_loader::load_requirements_toml;
pub(super) use crate::config_loader::version_for_toml;
pub(super) use praxis_config::CONFIG_TOML_FILE;
pub(super) use praxis_protocol::config_types::TrustLevel;
pub(super) use praxis_protocol::config_types::WebSearchMode;
pub(super) use praxis_protocol::protocol::AskForApproval;
#[cfg(target_os = "macos")]
pub(super) use praxis_protocol::protocol::SandboxPolicy;
pub(super) use praxis_utils_absolute_path::AbsolutePathBuf;
pub(super) use std::collections::BTreeMap;
pub(super) use std::collections::HashMap;
pub(super) use std::path::Path;
pub(super) use tempfile::tempdir;
pub(super) use toml::Value as TomlValue;

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

#[path = "tests/cloud_requirements.rs"]
mod cloud_requirements;
#[path = "tests/config_errors.rs"]
mod config_errors;
#[path = "tests/layer_merging.rs"]
mod layer_merging;
#[path = "tests/managed_preferences.rs"]
mod managed_preferences;
#[path = "tests/project_layers.rs"]
mod project_layers;
#[path = "tests/project_markers.rs"]
mod project_markers;
#[path = "tests/requirements_exec_policy.rs"]
mod requirements_exec_policy;
