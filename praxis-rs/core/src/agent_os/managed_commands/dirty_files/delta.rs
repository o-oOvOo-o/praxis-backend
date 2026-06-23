use std::collections::HashMap;
use std::collections::HashSet;
use std::path::Path;
use std::path::PathBuf;

use crate::agent_os::records::DirtyFileFingerprint;
use crate::path_scope::normalize_path_for_scope;

use super::fingerprint::dirty_file_fingerprint;

pub(in crate::agent_os) fn dirty_file_delta(
    cwd: &Path,
    before: &[PathBuf],
    before_fingerprints: &HashMap<String, DirtyFileFingerprint>,
    after: &[PathBuf],
) -> Vec<PathBuf> {
    let before = before
        .iter()
        .map(|path| normalize_path_for_scope(path))
        .collect::<HashSet<_>>();
    let mut seen = HashSet::new();
    after
        .iter()
        .filter_map(|path| {
            let normalized = normalize_path_for_scope(path);
            if !seen.insert(normalized.clone()) {
                return None;
            }
            if before.contains(&normalized) {
                let current = dirty_file_fingerprint(cwd, path);
                if before_fingerprints.get(&normalized) == Some(&current) {
                    return None;
                }
            }
            Some(path.clone())
        })
        .collect()
}

pub(in crate::agent_os) fn push_unique_dirty_files(
    target: &mut Vec<PathBuf>,
    dirty_files: &[PathBuf],
) {
    let mut seen = target
        .iter()
        .map(|path| normalize_path_for_scope(path))
        .collect::<HashSet<_>>();
    for path in dirty_files {
        if seen.insert(normalize_path_for_scope(path)) {
            target.push(path.clone());
        }
    }
}
