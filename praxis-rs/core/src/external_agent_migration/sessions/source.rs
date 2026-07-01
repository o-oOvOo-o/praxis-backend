// External-agent anti-corruption layer: the single source of truth for
// mapping external agent identities into Praxis session metadata.
//
// Each variant maps to a session provider module (cursor/, codex/)
// that handles source-specific detection, extraction, and conversion.
// The shared conversion helpers in convert.rs produce Praxis-native
// RolloutItems after per-source extraction.
//
// Claude does not have a session provider: its migration is config-only
// and lives in the parent config module.
use praxis_protocol::protocol::SessionMeta;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ExternalAgentSource {
    Codex,
    Cursor,
}

impl ExternalAgentSource {
    pub(super) const fn import_model_provider_id(self) -> &'static str {
        match self {
            Self::Codex => "codex",
            Self::Cursor => "cursor",
        }
    }

    pub const fn bridge_state_dir_name(self) -> &'static str {
        match self {
            Self::Codex => "codex_bridge_state",
            Self::Cursor => "cursor_bridge_state",
        }
    }

    pub const fn bridge_log_dir_name(self) -> &'static str {
        match self {
            Self::Codex => "codex_bridge",
            Self::Cursor => "cursor_bridge",
        }
    }

    pub(super) fn apply_session_meta_identity(self, meta: &mut SessionMeta) {
        let source_id = self.import_model_provider_id().to_string();
        meta.originator = source_id.clone();
        meta.model_provider = Some(source_id);
    }
}
