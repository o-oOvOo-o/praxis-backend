use super::Config;
use super::ConfigOverrides;
use super::ConfigToml;
use super::find_praxis_home;
use crate::config_loader::CloudConfigBundleLoader;
use crate::config_loader::CloudRequirementsLoader;
use crate::config_loader::LoaderOverrides;
use crate::config_loader::load_config_layers_state;
use praxis_utils_absolute_path::AbsolutePathBuf;
use std::path::PathBuf;
use toml::Value as TomlValue;

#[derive(Debug, Clone, Default)]
pub struct ConfigBuilder {
    praxis_home: Option<PathBuf>,
    cli_overrides: Option<Vec<(String, TomlValue)>>,
    harness_overrides: Option<ConfigOverrides>,
    loader_overrides: Option<LoaderOverrides>,
    cloud_config_bundle: CloudConfigBundleLoader,
    fallback_cwd: Option<PathBuf>,
}

impl ConfigBuilder {
    pub fn praxis_home(mut self, praxis_home: PathBuf) -> Self {
        self.praxis_home = Some(praxis_home);
        self
    }

    pub fn cli_overrides(mut self, cli_overrides: Vec<(String, TomlValue)>) -> Self {
        self.cli_overrides = Some(cli_overrides);
        self
    }

    pub fn harness_overrides(mut self, harness_overrides: ConfigOverrides) -> Self {
        self.harness_overrides = Some(harness_overrides);
        self
    }

    pub fn loader_overrides(mut self, loader_overrides: LoaderOverrides) -> Self {
        self.loader_overrides = Some(loader_overrides);
        self
    }

    pub fn cloud_config_bundle(mut self, cloud_config_bundle: CloudConfigBundleLoader) -> Self {
        self.cloud_config_bundle = cloud_config_bundle;
        self
    }

    pub fn cloud_requirements(mut self, cloud_requirements: CloudRequirementsLoader) -> Self {
        self.cloud_config_bundle = cloud_requirements.into();
        self
    }

    pub fn fallback_cwd(mut self, fallback_cwd: Option<PathBuf>) -> Self {
        self.fallback_cwd = fallback_cwd;
        self
    }

    pub async fn build(self) -> std::io::Result<Config> {
        let Self {
            praxis_home,
            cli_overrides,
            harness_overrides,
            loader_overrides,
            cloud_config_bundle,
            fallback_cwd,
        } = self;
        let praxis_home = praxis_home.map_or_else(find_praxis_home, std::io::Result::Ok)?;
        let cli_overrides = cli_overrides.unwrap_or_default();
        let mut harness_overrides = harness_overrides.unwrap_or_default();
        let loader_overrides = loader_overrides.unwrap_or_default();
        let cwd_override = harness_overrides.cwd.as_deref().or(fallback_cwd.as_deref());
        let cwd = match cwd_override {
            Some(path) => AbsolutePathBuf::relative_to_current_dir(path)?,
            None => AbsolutePathBuf::current_dir()?,
        };
        harness_overrides.cwd = Some(cwd.to_path_buf());
        let config_layer_stack = load_config_layers_state(
            &praxis_home,
            Some(cwd),
            &cli_overrides,
            loader_overrides,
            cloud_config_bundle,
        )
        .await?;
        let merged_toml = config_layer_stack.effective_config();

        let config_toml: ConfigToml = match merged_toml.try_into() {
            Ok(config_toml) => config_toml,
            Err(err) => {
                if let Some(config_error) =
                    crate::config_loader::first_layer_config_error(&config_layer_stack).await
                {
                    return Err(crate::config_loader::io_error_from_config_error(
                        std::io::ErrorKind::InvalidData,
                        config_error,
                        Some(err),
                    ));
                }
                return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, err));
            }
        };
        Config::load_config_with_layer_stack(
            config_toml,
            harness_overrides,
            praxis_home,
            config_layer_stack,
        )
    }
}
