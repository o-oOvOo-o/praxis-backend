use praxis_config::CONFIG_TOML_FILE;
use praxis_features::unstable_features_warning_event;
use praxis_protocol::protocol::Event;
use toml::Value as TomlValue;

use crate::config::Config;

pub(super) fn append(events: &mut Vec<Event>, config: &Config) {
    let config_path = config.praxis_home.join(CONFIG_TOML_FILE);
    if let Some(event) = unstable_features_warning_event(
        config
            .config_layer_stack
            .effective_config()
            .get("features")
            .and_then(TomlValue::as_table),
        config.suppress_unstable_features_warning,
        &config.features,
        &config_path.display().to_string(),
    ) {
        events.push(event);
    }
}
