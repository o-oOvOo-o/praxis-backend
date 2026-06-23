use serde::Deserialize;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalCacheScope {
    Global,
    Thread,
    Turn,
}

impl Default for ApprovalCacheScope {
    fn default() -> Self {
        Self::Thread
    }
}
