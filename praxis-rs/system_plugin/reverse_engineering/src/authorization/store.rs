use super::AuthorizationScope;
use crate::ReverseError;
use praxis_utils_time::unix_timestamp_seconds;
use std::collections::HashSet;
use std::fs::OpenOptions;
use std::io::BufRead;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;

const SCOPES_DIR: &str = "authorization";
const SCOPES_FILE: &str = "scopes.jsonl";

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ScopeRecord {
    Granted {
        scope: AuthorizationScope,
    },
    Revoked {
        scope_id: String,
        revoked_at_unix: i64,
    },
}

pub fn append_granted(root: &Path, scope: &AuthorizationScope) -> Result<(), ReverseError> {
    append(
        root,
        &ScopeRecord::Granted {
            scope: scope.clone(),
        },
    )
}

pub fn append_revoked(root: &Path, scope_id: &str) -> Result<(), ReverseError> {
    append(
        root,
        &ScopeRecord::Revoked {
            scope_id: scope_id.to_string(),
            revoked_at_unix: unix_timestamp_seconds(),
        },
    )
}

pub fn load_active_scope(root: &Path, scope_id: &str) -> Result<AuthorizationScope, ReverseError> {
    let path = scopes_path(root);
    if !path.is_file() {
        return Err(ReverseError::Authorization(format!(
            "scope {scope_id} was not found"
        )));
    }
    let file = std::fs::File::open(&path).map_err(|err| ReverseError::io(&path, err))?;
    let mut latest = None;
    let mut revoked = HashSet::new();
    for line in std::io::BufReader::new(file).lines() {
        let line = line.map_err(|err| ReverseError::io(&path, err))?;
        if line.trim().is_empty() {
            continue;
        }
        let record: ScopeRecord =
            serde_json::from_str(&line).map_err(|err| ReverseError::json(&path, err))?;
        match record {
            ScopeRecord::Granted { scope } if scope.scope_id == scope_id => {
                latest = Some(scope);
            }
            ScopeRecord::Revoked {
                scope_id: revoked_id,
                ..
            } if revoked_id == scope_id => {
                revoked.insert(revoked_id);
            }
            _ => {}
        }
    }
    if revoked.contains(scope_id) {
        return Err(ReverseError::Authorization(format!(
            "scope {scope_id} has been revoked"
        )));
    }
    latest.ok_or_else(|| ReverseError::Authorization(format!("scope {scope_id} was not found")))
}

fn append(root: &Path, record: &ScopeRecord) -> Result<(), ReverseError> {
    let path = scopes_path(root);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|err| ReverseError::io(parent, err))?;
    }
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|err| ReverseError::io(&path, err))?;
    serde_json::to_writer(&mut file, record).map_err(|err| ReverseError::json(&path, err))?;
    file.write_all(b"\n")
        .map_err(|err| ReverseError::io(&path, err))?;
    Ok(())
}

fn scopes_path(root: &Path) -> PathBuf {
    root.join(SCOPES_DIR).join(SCOPES_FILE)
}
