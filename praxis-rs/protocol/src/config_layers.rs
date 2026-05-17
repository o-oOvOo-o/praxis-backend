use praxis_utils_absolute_path::AbsolutePathBuf;
use schemars::JsonSchema;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value as JsonValue;
use ts_rs::TS;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, JsonSchema, TS)]
#[serde(tag = "type", rename_all = "camelCase")]
#[ts(tag = "type")]
pub enum ConfigLayerSource {
    /// Managed preferences layer delivered by MDM (macOS only).
    #[serde(rename_all = "camelCase")]
    #[ts(rename_all = "camelCase")]
    Mdm {
        domain: String,
        key: String,
    },

    /// Managed config layer from a file (usually `managed_config.toml`).
    #[serde(rename_all = "camelCase")]
    #[ts(rename_all = "camelCase")]
    System {
        /// This is the path to the system config.toml file, though it is not guaranteed to exist.
        file: AbsolutePathBuf,
    },

    /// User config layer from $PRAXIS_HOME/config.toml.
    #[serde(rename_all = "camelCase")]
    #[ts(rename_all = "camelCase")]
    User {
        /// This is the path to the user's config.toml file, though it is not guaranteed to exist.
        file: AbsolutePathBuf,
    },

    /// Path to a .praxis/ folder within a project.
    #[serde(rename_all = "camelCase")]
    #[ts(rename_all = "camelCase")]
    Project {
        dot_praxis_folder: AbsolutePathBuf,
    },

    /// Session-layer overrides supplied via `-c`/`--config`.
    SessionFlags,

    /// Legacy managed config file layer preserved for requirements migration.
    #[serde(rename_all = "camelCase")]
    #[ts(rename_all = "camelCase")]
    LegacyManagedConfigTomlFromFile {
        file: AbsolutePathBuf,
    },

    LegacyManagedConfigTomlFromMdm,
}

impl ConfigLayerSource {
    /// Settings from a higher-precedence layer override settings from a lower-precedence layer.
    pub fn precedence(&self) -> i16 {
        match self {
            ConfigLayerSource::Mdm { .. } => 0,
            ConfigLayerSource::System { .. } => 10,
            ConfigLayerSource::User { .. } => 20,
            ConfigLayerSource::Project { .. } => 25,
            ConfigLayerSource::SessionFlags => 30,
            ConfigLayerSource::LegacyManagedConfigTomlFromFile { .. } => 40,
            ConfigLayerSource::LegacyManagedConfigTomlFromMdm => 50,
        }
    }
}

impl PartialOrd for ConfigLayerSource {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.precedence().cmp(&other.precedence()))
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct ConfigLayerMetadata {
    pub name: ConfigLayerSource,
    pub version: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct ConfigLayer {
    pub name: ConfigLayerSource,
    pub version: String,
    pub config: JsonValue,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disabled_reason: Option<String>,
}
