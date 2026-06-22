use serde::Deserialize;
use serde::Serialize;

use super::ConcurrencyMode;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    pub concurrency: ConcurrencyMode,
}

impl ToolSpec {
    pub fn new(name: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            concurrency: ConcurrencyMode::Parallel,
        }
    }
}
