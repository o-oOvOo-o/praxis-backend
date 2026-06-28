use super::Action;
use super::AuthorizationScope;
use super::store;
use crate::ReverseError;
use std::path::Path;

pub fn require_scope(
    artifact_root: &Path,
    scope_id: &str,
    action: Action,
) -> Result<AuthorizationScope, ReverseError> {
    let scope = store::load_active_scope(artifact_root, scope_id)?;
    scope.require_action(action)?;
    Ok(scope)
}
