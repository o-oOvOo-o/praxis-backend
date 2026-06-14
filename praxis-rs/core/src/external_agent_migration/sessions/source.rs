#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ExternalAgentSource {
    Codex,
    Cursor,
}

impl ExternalAgentSource {
    pub fn provider_id(self) -> &'static str {
        match self {
            Self::Codex => "codex",
            Self::Cursor => "cursor",
        }
    }

    pub fn bridge_state_dir_name(self) -> &'static str {
        match self {
            Self::Codex => "codex_bridge_state",
            Self::Cursor => "cursor_bridge_state",
        }
    }

    pub fn bridge_log_dir_name(self) -> &'static str {
        match self {
            Self::Codex => "codex_bridge",
            Self::Cursor => "cursor_bridge",
        }
    }
}
