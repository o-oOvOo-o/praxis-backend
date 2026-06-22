use super::claude;
use std::collections::HashSet;
use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::Path;
use std::path::PathBuf;

pub(super) fn home_target(praxis_home: &Path) -> PathBuf {
    praxis_home
        .parent()
        .map(|parent| parent.join(".agents").join("skills"))
        .unwrap_or_else(|| PathBuf::from(".agents").join("skills"))
}

pub(super) fn repo_target(repo_root: &Path) -> PathBuf {
    repo_root.join(".agents").join("skills")
}

pub(super) fn count_missing(source: &Path, target: &Path) -> io::Result<usize> {
    let source_names = collect_subdirectory_names(source)?;
    let target_names = collect_subdirectory_names(target)?;
    Ok(source_names
        .iter()
        .filter(|name| !target_names.contains(*name))
        .count())
}

pub(super) fn import_missing(source: &Path, target: &Path) -> io::Result<usize> {
    if !source.is_dir() {
        return Ok(0);
    }

    fs::create_dir_all(target)?;
    let mut copied_count = 0usize;

    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        if !file_type.is_dir() {
            continue;
        }

        let target = target.join(entry.file_name());
        if target.exists() {
            continue;
        }

        copy_dir_recursive(&entry.path(), &target)?;
        copied_count += 1;
    }

    Ok(copied_count)
}

fn collect_subdirectory_names(path: &Path) -> io::Result<HashSet<OsString>> {
    let mut names = HashSet::new();
    if !path.is_dir() {
        return Ok(names);
    }

    for entry in fs::read_dir(path)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            names.insert(entry.file_name());
        }
    }

    Ok(names)
}

fn copy_dir_recursive(source: &Path, target: &Path) -> io::Result<()> {
    fs::create_dir_all(target)?;

    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let source_path = entry.path();
        let target_path = target.join(entry.file_name());
        let file_type = entry.file_type()?;

        if file_type.is_dir() {
            copy_dir_recursive(&source_path, &target_path)?;
            continue;
        }

        if file_type.is_file() {
            if is_skill_md(&source_path) {
                claude::rewrite_and_copy_text_file(&source_path, &target_path)?;
            } else {
                fs::copy(source_path, target_path)?;
            }
        }
    }

    Ok(())
}

fn is_skill_md(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.eq_ignore_ascii_case("SKILL.md"))
}
