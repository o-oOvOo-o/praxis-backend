use crate::config::Config;
use crate::config::ConfigOverrides;
use crate::config::deserialize_config_toml_with_base;
use crate::config_loader::ConfigLayerEntry;
use crate::config_loader::ConfigLayerStack;
use crate::config_loader::ConfigLayerStackOrdering;
use praxis_protocol::config_layers::ConfigLayerSource;
use toml::Value as TomlValue;

pub(super) fn build_next_config(
    config: &Config,
    role_layer_toml: TomlValue,
    preserve_current_profile: bool,
    preserve_current_provider: bool,
) -> anyhow::Result<Config> {
    let active_profile_name = preserve_current_profile
        .then_some(config.active_profile.as_deref())
        .flatten();
    let config_layer_stack =
        build_config_layer_stack(config, &role_layer_toml, active_profile_name)?;
    let mut merged_config = deserialize_effective_config(config, &config_layer_stack)?;
    if preserve_current_profile {
        merged_config.profile = None;
    }

    let mut next_config = Config::load_config_with_layer_stack(
        merged_config,
        reload_overrides(config, preserve_current_provider),
        config.praxis_home.clone(),
        config_layer_stack,
    )?;
    if preserve_current_profile {
        next_config.active_profile = config.active_profile.clone();
    }
    Ok(next_config)
}

fn build_config_layer_stack(
    config: &Config,
    role_layer_toml: &TomlValue,
    active_profile_name: Option<&str>,
) -> anyhow::Result<ConfigLayerStack> {
    let mut layers = existing_layers(config);
    if let Some(resolved_profile_layer) =
        resolved_profile_layer(config, &layers, role_layer_toml, active_profile_name)?
    {
        insert_layer(&mut layers, resolved_profile_layer);
    }
    insert_layer(&mut layers, role_layer(role_layer_toml.clone()));
    Ok(ConfigLayerStack::new(
        layers,
        config.config_layer_stack.requirements().clone(),
        config.config_layer_stack.requirements_toml().clone(),
    )?)
}

fn resolved_profile_layer(
    config: &Config,
    existing_layers: &[ConfigLayerEntry],
    role_layer_toml: &TomlValue,
    active_profile_name: Option<&str>,
) -> anyhow::Result<Option<ConfigLayerEntry>> {
    let Some(active_profile_name) = active_profile_name else {
        return Ok(None);
    };

    let mut layers = existing_layers.to_vec();
    insert_layer(&mut layers, role_layer(role_layer_toml.clone()));
    let merged_config = deserialize_effective_config(
        config,
        &ConfigLayerStack::new(
            layers,
            config.config_layer_stack.requirements().clone(),
            config.config_layer_stack.requirements_toml().clone(),
        )?,
    )?;
    let resolved_profile =
        merged_config.get_config_profile(Some(active_profile_name.to_string()))?;
    Ok(Some(ConfigLayerEntry::new(
        ConfigLayerSource::SessionFlags,
        TomlValue::try_from(resolved_profile)?,
    )))
}

fn deserialize_effective_config(
    config: &Config,
    config_layer_stack: &ConfigLayerStack,
) -> anyhow::Result<crate::config::ConfigToml> {
    Ok(deserialize_config_toml_with_base(
        config_layer_stack.effective_config(),
        &config.praxis_home,
    )?)
}

fn existing_layers(config: &Config) -> Vec<ConfigLayerEntry> {
    config
        .config_layer_stack
        .get_layers(
            ConfigLayerStackOrdering::LowestPrecedenceFirst,
            /*include_disabled*/ true,
        )
        .into_iter()
        .cloned()
        .collect()
}

fn insert_layer(layers: &mut Vec<ConfigLayerEntry>, layer: ConfigLayerEntry) {
    let insertion_index =
        layers.partition_point(|existing_layer| existing_layer.name <= layer.name);
    layers.insert(insertion_index, layer);
}

fn role_layer(role_layer_toml: TomlValue) -> ConfigLayerEntry {
    ConfigLayerEntry::new(ConfigLayerSource::SessionFlags, role_layer_toml)
}

fn reload_overrides(config: &Config, preserve_current_provider: bool) -> ConfigOverrides {
    ConfigOverrides {
        cwd: Some(config.cwd.to_path_buf()),
        model_provider: preserve_current_provider.then(|| config.model_provider_id.clone()),
        praxis_linux_sandbox_exe: config.praxis_linux_sandbox_exe.clone(),
        main_execve_wrapper_exe: config.main_execve_wrapper_exe.clone(),
        ..Default::default()
    }
}
