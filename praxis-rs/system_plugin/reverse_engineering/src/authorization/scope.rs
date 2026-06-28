use std::path::PathBuf;

use praxis_utils_time::unix_timestamp_seconds;

#[derive(Debug, Clone, Copy, serde::Deserialize, serde::Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TargetKind {
    Native,
    ManagedDotNet,
    ManagedJvm,
    Shader,
    Unity,
    Other,
}

#[derive(Debug, Clone, Copy, serde::Deserialize, serde::Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    Fingerprint,
    Ingest,
    ExtractStatic,
    Decompile,
    ShaderReflect,
    ProbeBlackbox,
    CompareBehavior,
    RecordEvidence,
    Harden,
}

#[derive(Debug, Clone, Copy, Default, serde::Deserialize, serde::Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AuthorizationLevel {
    #[default]
    ScopedAnalysis,
    FullDecompilation,
    OwnedHardening,
}

#[derive(Debug, Clone, Copy, Default, serde::Deserialize, serde::Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ModelExposurePolicy {
    #[default]
    CodecProjectionOnly,
}

#[derive(Debug, Clone, Copy, Default, serde::Deserialize, serde::Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LocalRawAccess {
    None,
    #[default]
    ScopedArtifacts,
    FullDecompilerArtifacts,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, PartialEq, Eq)]
pub struct AuthorizationScope {
    pub scope_id: String,
    pub target_hash: String,
    pub target_path: PathBuf,
    pub target_kind: TargetKind,
    #[serde(default)]
    pub authorization_level: AuthorizationLevel,
    pub authorization_note: String,
    pub allowed_actions: Vec<Action>,
    pub forbidden_actions: Vec<Action>,
    #[serde(default)]
    pub model_exposure_policy: ModelExposurePolicy,
    #[serde(default)]
    pub local_raw_access: LocalRawAccess,
    pub expires_at_unix: i64,
    pub granted_at_unix: i64,
    pub granted_by: String,
    pub artifact_root: PathBuf,
}

impl AuthorizationScope {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        target_hash: String,
        target_path: PathBuf,
        target_kind: TargetKind,
        authorization_level: AuthorizationLevel,
        authorization_note: String,
        mut allowed_actions: Vec<Action>,
        forbidden_actions: Vec<Action>,
        expires_after_secs: Option<i64>,
        granted_by: String,
        artifact_root: PathBuf,
    ) -> Self {
        if allowed_actions.is_empty() {
            allowed_actions = default_actions(authorization_level);
        }
        let granted_at_unix = unix_timestamp_seconds();
        let expires_at_unix = granted_at_unix + expires_after_secs.unwrap_or(60 * 60 * 8);
        let scope_id = scope_id(&target_hash, &target_path, granted_at_unix);
        Self {
            scope_id,
            target_hash,
            target_path,
            target_kind,
            authorization_level,
            authorization_note,
            allowed_actions,
            forbidden_actions,
            model_exposure_policy: ModelExposurePolicy::CodecProjectionOnly,
            local_raw_access: local_raw_access(authorization_level),
            expires_at_unix,
            granted_at_unix,
            granted_by,
            artifact_root,
        }
    }

    pub fn require_action(&self, action: Action) -> Result<(), crate::ReverseError> {
        if self.expires_at_unix < unix_timestamp_seconds() {
            return Err(crate::ReverseError::Authorization(format!(
                "scope {} has expired",
                self.scope_id
            )));
        }
        if self.forbidden_actions.contains(&action) {
            return Err(crate::ReverseError::Authorization(format!(
                "action {action:?} is explicitly forbidden by scope {}",
                self.scope_id
            )));
        }
        if !self.allowed_actions.contains(&action) {
            return Err(crate::ReverseError::Authorization(format!(
                "action {action:?} is not allowed by scope {}",
                self.scope_id
            )));
        }
        Ok(())
    }
}

fn default_actions(level: AuthorizationLevel) -> Vec<Action> {
    let mut actions = vec![
        Action::Fingerprint,
        Action::Ingest,
        Action::ExtractStatic,
        Action::Decompile,
        Action::ShaderReflect,
        Action::CompareBehavior,
        Action::RecordEvidence,
    ];
    match level {
        AuthorizationLevel::ScopedAnalysis => {}
        AuthorizationLevel::FullDecompilation => {
            actions.push(Action::ProbeBlackbox);
        }
        AuthorizationLevel::OwnedHardening => {
            actions.push(Action::ProbeBlackbox);
            actions.push(Action::Harden);
        }
    }
    actions
}

fn local_raw_access(level: AuthorizationLevel) -> LocalRawAccess {
    match level {
        AuthorizationLevel::ScopedAnalysis => LocalRawAccess::ScopedArtifacts,
        AuthorizationLevel::FullDecompilation | AuthorizationLevel::OwnedHardening => {
            LocalRawAccess::FullDecompilerArtifacts
        }
    }
}

fn scope_id(target_hash: &str, target_path: &std::path::Path, granted_at_unix: i64) -> String {
    let timestamp = granted_at_unix.to_le_bytes();
    let path = target_path.to_string_lossy();
    crate::hash_util::short_id(
        "re_scope",
        &[target_hash.as_bytes(), path.as_bytes(), &timestamp],
    )
}
