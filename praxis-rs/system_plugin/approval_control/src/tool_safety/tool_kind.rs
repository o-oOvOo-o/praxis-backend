use serde::Deserialize;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SafetyToolKind {
    Exec,
    ApplyPatch,
    Mcp,
    Network,
    Permission,
    Unknown,
}
