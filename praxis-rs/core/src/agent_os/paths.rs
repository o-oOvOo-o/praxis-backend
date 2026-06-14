use super::ActionIntentKind;
use crate::path_scope::normalize_path_for_scope;
use std::collections::hash_map::DefaultHasher;
use std::hash::Hash;
use std::hash::Hasher;
use std::path::Path;
use std::path::PathBuf;

pub(super) fn action_fingerprint(
    command: &[String],
    cwd: &Path,
    intent: ActionIntentKind,
) -> String {
    let mut hasher = DefaultHasher::new();
    intent.hash(&mut hasher);
    normalize_path_for_scope(&stable_path(cwd)).hash(&mut hasher);
    for arg in command {
        arg.hash(&mut hasher);
    }
    format!("{:016x}", hasher.finish())
}

pub(super) fn repo_scope_for_cwd(cwd: &Path) -> String {
    let root = find_repo_root(cwd).unwrap_or_else(|| stable_path(cwd));
    let normalized = normalize_path_for_scope(&root);
    let mut hasher = DefaultHasher::new();
    normalized.hash(&mut hasher);
    let name = root
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or("repo");
    format!("repo:{name}:{:016x}", hasher.finish())
}

fn stable_path(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

pub(super) fn find_repo_root(cwd: &Path) -> Option<PathBuf> {
    let mut current = if cwd.is_file() {
        cwd.parent()?.to_path_buf()
    } else {
        cwd.to_path_buf()
    };
    loop {
        if current.join(".git").exists() {
            return Some(stable_path(&current));
        }
        if !current.pop() {
            return None;
        }
    }
}
