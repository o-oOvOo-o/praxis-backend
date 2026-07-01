use super::claude;
use std::fs;
use std::io;
use std::path::Path;
use std::path::PathBuf;

pub(super) fn home_target(praxis_home: &Path) -> PathBuf {
    praxis_home.join("AGENTS.md")
}

pub(super) fn repo_target(repo_root: &Path) -> PathBuf {
    repo_root.join("AGENTS.md")
}

pub(super) fn home_source(claude_home: &Path) -> io::Result<Option<PathBuf>> {
    let path = claude::home_agents_md(claude_home);
    is_non_empty_text_file(&path).map(|exists| exists.then_some(path))
}

pub(super) fn repo_source(repo_root: &Path) -> io::Result<Option<PathBuf>> {
    for candidate in claude::repo_agents_md_candidates(repo_root) {
        if is_non_empty_text_file(&candidate)? {
            return Ok(Some(candidate));
        }
    }

    Ok(None)
}

pub(super) fn target_needs_import(path: &Path) -> io::Result<bool> {
    is_missing_or_empty_text_file(path)
}

pub(super) fn import(source: &Path, target: &Path) -> io::Result<()> {
    if !is_non_empty_text_file(source)? || !is_missing_or_empty_text_file(target)? {
        return Ok(());
    }

    let Some(target_parent) = target.parent() else {
        return Err(super::invalid_data_error(
            "AGENTS.md target path has no parent",
        ));
    };
    fs::create_dir_all(target_parent)?;

    claude::rewrite_and_copy_text_file(source, target)
}

fn is_missing_or_empty_text_file(path: &Path) -> io::Result<bool> {
    if !path.exists() {
        return Ok(true);
    }
    if !path.is_file() {
        return Ok(false);
    }

    Ok(fs::read_to_string(path)?.trim().is_empty())
}

fn is_non_empty_text_file(path: &Path) -> io::Result<bool> {
    if !path.is_file() {
        return Ok(false);
    }

    Ok(!fs::read_to_string(path)?.trim().is_empty())
}
