use serde::Deserialize;
use serde::Serialize;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ModelSpec {
    pub slug: String,
    pub provider_id: Option<String>,
    pub context_window: Option<u64>,
    pub input_modalities: Vec<String>,
}

impl ModelSpec {
    pub fn new(slug: impl Into<String>) -> Self {
        Self {
            slug: slug.into(),
            provider_id: None,
            context_window: None,
            input_modalities: Vec::new(),
        }
    }
}
