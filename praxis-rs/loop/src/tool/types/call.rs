use std::collections::BTreeMap;

use serde::Deserialize;
use serde::Serialize;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub namespace: Option<String>,
    pub arguments: String,
    pub metadata: BTreeMap<String, String>,
}

impl ToolCall {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            namespace: None,
            arguments: String::new(),
            metadata: BTreeMap::new(),
        }
    }
}
