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

#[derive(Debug, Clone, Copy)]
struct ExternalAgentSourceIdentity {
    import_model_provider_id: &'static str,
    session_originator_id: &'static str,
    bridge_state_dir_name: &'static str,
    bridge_log_dir_name: &'static str,
}

impl ExternalAgentSourceIdentity {
    const fn new(
        import_model_provider_id: &'static str,
        session_originator_id: &'static str,
        bridge_state_dir_name: &'static str,
        bridge_log_dir_name: &'static str,
    ) -> Self {
        Self {
            import_model_provider_id,
            session_originator_id,
            bridge_state_dir_name,
            bridge_log_dir_name,
        }
    }

    const fn uniform(
        source_id: &'static str,
        bridge_state_dir_name: &'static str,
        bridge_log_dir_name: &'static str,
    ) -> Self {
        Self::new(
            source_id,
            source_id,
            bridge_state_dir_name,
            bridge_log_dir_name,
        )
    }

    fn apply_to_session_meta(self, meta: &mut SessionMeta) {
        meta.originator = self.session_originator_id.to_string();
        meta.model_provider = Some(self.import_model_provider_id.to_string());
    }
}

const CODEX_IDENTITY: ExternalAgentSourceIdentity =
    ExternalAgentSourceIdentity::uniform("codex", "codex_bridge_state", "codex_bridge");
const CURSOR_IDENTITY: ExternalAgentSourceIdentity =
    ExternalAgentSourceIdentity::uniform("cursor", "cursor_bridge_state", "cursor_bridge");

impl ExternalAgentSource {
    const fn identity(self) -> ExternalAgentSourceIdentity {
        match self {
            Self::Codex => CODEX_IDENTITY,
            Self::Cursor => CURSOR_IDENTITY,
        }
    }

    pub(super) const fn import_model_provider_id(self) -> &'static str {
        self.identity().import_model_provider_id
    }

    pub const fn bridge_state_dir_name(self) -> &'static str {
        self.identity().bridge_state_dir_name
    }

    pub const fn bridge_log_dir_name(self) -> &'static str {
        self.identity().bridge_log_dir_name
    }

    pub(super) fn apply_session_meta_identity(self, meta: &mut SessionMeta) {
        self.identity().apply_to_session_meta(meta);
    }
}
