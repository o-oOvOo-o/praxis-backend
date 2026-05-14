use praxis_core::models_manager::collaboration_mode_presets::CollaborationModesConfig;
use praxis_core::models_manager::collaboration_mode_presets::builtin_collaboration_mode_presets;
use praxis_protocol::config_types::CollaborationModeMask;
use praxis_protocol::openai_models::ModelPreset;
use std::collections::HashMap;
use std::convert::Infallible;

use crate::model_discovery::ModelCatalogSelectionMetadata;

#[derive(Debug, Clone)]
pub(crate) struct ModelCatalog {
    models: Vec<ModelPreset>,
    metadata_by_preset_id: HashMap<String, ModelCatalogSelectionMetadata>,
    collaboration_modes_config: CollaborationModesConfig,
}

impl ModelCatalog {
    pub(crate) fn new(
        models: Vec<ModelPreset>,
        metadata_by_preset_id: HashMap<String, ModelCatalogSelectionMetadata>,
        collaboration_modes_config: CollaborationModesConfig,
    ) -> Self {
        Self {
            models,
            metadata_by_preset_id,
            collaboration_modes_config,
        }
    }

    pub(crate) fn try_list_models(&self) -> Result<Vec<ModelPreset>, Infallible> {
        Ok(self.models.clone())
    }

    pub(crate) fn list_collaboration_modes(&self) -> Vec<CollaborationModeMask> {
        builtin_collaboration_mode_presets(self.collaboration_modes_config)
    }

    pub(crate) fn selection_metadata(
        &self,
        preset_id: &str,
    ) -> Option<&ModelCatalogSelectionMetadata> {
        self.metadata_by_preset_id.get(preset_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn list_collaboration_modes_matches_core_presets() {
        let collaboration_modes_config = CollaborationModesConfig {
            default_mode_request_user_input: true,
        };
        let catalog = ModelCatalog::new(Vec::new(), HashMap::new(), collaboration_modes_config);

        assert_eq!(
            catalog.list_collaboration_modes(),
            builtin_collaboration_mode_presets(collaboration_modes_config)
        );
    }
}
