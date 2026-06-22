use praxis_config::CONFIG_TOML_FILE;
use praxis_features::unstable_features_warning_event;
use praxis_protocol::protocol::AskForApproval;
use praxis_protocol::protocol::Event;
use toml::Value as TomlValue;

use crate::config::Config;
use crate::config::uses_deprecated_instructions_file;
use crate::praxis::INITIAL_SUBMIT_ID;
use crate::praxis::event_delivery::make_deprecation_notice_event;
use crate::praxis::event_delivery::make_warning_event;

pub(super) fn build_post_configured_events(config: &Config) -> Vec<Event> {
    let mut events = Vec::new();

    for usage in config.features.legacy_feature_usages() {
        events.push(make_deprecation_notice_event(
            INITIAL_SUBMIT_ID,
            usage.summary.clone(),
            usage.details.clone(),
        ));
    }
    if uses_deprecated_instructions_file(&config.config_layer_stack) {
        events.push(make_deprecation_notice_event(
            INITIAL_SUBMIT_ID,
            "`experimental_instructions_file` is deprecated and ignored. Use `model_instructions_file` instead.",
            Some(
                "Move the setting to `model_instructions_file` in config.toml (or under a profile) to load instructions from a file."
                    .to_string(),
            ),
        ));
    }
    for message in &config.startup_warnings {
        events.push(make_warning_event("", message.clone()));
    }
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
    if config.permissions.approval_policy.value() == AskForApproval::OnFailure {
        events.push(make_warning_event(
            "",
            "`on-failure` approval policy is deprecated and will be removed in a future release. Use `on-request` for interactive approvals or `never` for non-interactive runs.",
        ));
    }

    events
}

pub(super) fn hook_warning_event(warning: String) -> Event {
    make_warning_event(INITIAL_SUBMIT_ID, warning)
}
