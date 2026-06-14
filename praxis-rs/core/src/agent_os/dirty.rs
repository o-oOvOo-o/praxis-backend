use super::DirtyFileFingerprint;
use super::TaskRecord;
use super::paths::find_repo_root;
use crate::path_scope::normalize_path_for_scope;
use crate::path_scope::scope_matches;
use std::collections::HashMap;
use std::collections::HashSet;
use std::path::Path;
use std::path::PathBuf;

pub(super) fn dirty_file_allowed_by_task(task: &TaskRecord, path: &Path) -> bool {
    if task.exploratory || task.scope.is_empty() {
        return true;
    }
    let value = normalize_path_for_scope(path);
    task.scope
        .iter()
        .any(|pattern| scope_matches(pattern, &value))
}

pub(super) fn dirty_file_delta(
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

pub(super) fn push_unique_dirty_files(target: &mut Vec<PathBuf>, dirty_files: &[PathBuf]) {
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

pub(super) async fn audit_git_dirty_files(cwd: &Path) -> Vec<PathBuf> {
    let repo_root = find_repo_root(cwd);
    let output = tokio::process::Command::new("git")
        .arg("-C")
        .arg(cwd)
        .arg("status")
        .arg("--porcelain=v1")
        .arg("-z")
        .output()
        .await;
    let Ok(output) = output else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }
    parse_git_status_porcelain_z(&output.stdout)
        .into_iter()
        .map(|path| {
            if path.is_absolute() {
                path
            } else if let Some(root) = repo_root.as_ref() {
                root.join(path)
            } else {
                cwd.join(path)
            }
        })
        .collect()
}

pub(super) fn dirty_file_fingerprints(
    cwd: &Path,
    dirty_files: &[PathBuf],
) -> HashMap<String, DirtyFileFingerprint> {
    dirty_files
        .iter()
        .map(|path| {
            (
                normalize_path_for_scope(path),
                dirty_file_fingerprint(cwd, path),
            )
        })
        .collect()
}

fn dirty_file_fingerprint(cwd: &Path, path: &Path) -> DirtyFileFingerprint {
    let path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        cwd.join(path)
    };
    let Ok(metadata) = std::fs::metadata(path) else {
        return DirtyFileFingerprint {
            exists: false,
            len: None,
            modified_unix_millis: None,
        };
    };
    let modified_unix_millis = metadata
        .modified()
        .ok()
        .and_then(|modified| modified.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|duration| duration.as_millis() as i128);
    DirtyFileFingerprint {
        exists: true,
        len: Some(metadata.len()),
        modified_unix_millis,
    }
}

fn parse_git_status_porcelain_z(output: &[u8]) -> Vec<PathBuf> {
    let entries = output
        .split(|byte| *byte == 0)
        .filter(|entry| !entry.is_empty())
        .collect::<Vec<_>>();
    let mut paths = Vec::new();
    let mut idx = 0;
    while idx < entries.len() {
        let entry = entries[idx];
        if entry.len() < 4 || entry[2] != b' ' {
            idx += 1;
            continue;
        }
        let status = entry[0];
        let path = String::from_utf8_lossy(&entry[3..]).to_string();
        if !path.is_empty() {
            paths.push(PathBuf::from(path));
        }
        idx += if matches!(status, b'R' | b'C') { 2 } else { 1 };
    }
    paths
}

pub(super) fn format_dirty_file_report(
    dirty_files: &[PathBuf],
    violation_path: Option<&PathBuf>,
) -> String {
    let mut report = if dirty_files.is_empty() {
        "No dirty files detected.".to_string()
    } else {
        dirty_files
            .iter()
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>()
            .join("\n")
    };
    if let Some(path) = violation_path {
        report.push_str("\n\nPolicy violation: dirty file outside task/profile scope: ");
        report.push_str(path.display().to_string().as_str());
    }
    report
}
