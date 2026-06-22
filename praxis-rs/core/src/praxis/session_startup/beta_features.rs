use praxis_features::FEATURES;

use crate::config::Config;

pub(super) fn model_client_beta_features_header(config: &Config) -> Option<String> {
    let beta_features_header = FEATURES
        .iter()
        .filter_map(|spec| {
            if spec.stage.experimental_menu_description().is_some()
                && config.features.enabled(spec.id)
            {
                Some(spec.key)
            } else {
                None
            }
        })
        .collect::<Vec<_>>()
        .join(",");

    (!beta_features_header.is_empty()).then_some(beta_features_header)
}
