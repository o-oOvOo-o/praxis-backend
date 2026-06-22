use std::path::Path;
use std::path::PathBuf;

use crate::agent_os::paths::find_repo_root;

pub(in crate::agent_os) async fn audit_git_dirty_files(cwd: &Path) -> Vec<PathBuf> {
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
