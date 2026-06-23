use praxis_protocol::protocol::Event;

use crate::config::Config;
use crate::config::uses_deprecated_instructions_file;
use crate::praxis::INITIAL_SUBMIT_ID;
use crate::praxis::event_delivery::make_deprecation_notice_event;

pub(super) fn append(events: &mut Vec<Event>, config: &Config) {
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
}
