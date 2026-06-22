use praxis_git_utils::GhostSnapshotReport;

pub(super) fn format_snapshot_warnings(
    ignore_large_untracked_files: Option<i64>,
    ignore_large_untracked_dirs: Option<i64>,
    report: &GhostSnapshotReport,
) -> Vec<String> {
    let mut warnings = Vec::new();
    if let Some(message) = format_large_untracked_warning(ignore_large_untracked_dirs, report) {
        warnings.push(message);
    }
    if let Some(message) =
        format_ignored_untracked_files_warning(ignore_large_untracked_files, report)
    {
        warnings.push(message);
    }
    warnings
}

pub(super) fn format_large_untracked_warning(
    ignore_large_untracked_dirs: Option<i64>,
    report: &GhostSnapshotReport,
) -> Option<String> {
    if report.large_untracked_dirs.is_empty() {
        return None;
    }
    let threshold = ignore_large_untracked_dirs?;
    const MAX_DIRS: usize = 3;
    let mut parts: Vec<String> = Vec::new();
    for dir in report.large_untracked_dirs.iter().take(MAX_DIRS) {
        parts.push(format!("{} ({} files)", dir.path.display(), dir.file_count));
    }
    if report.large_untracked_dirs.len() > MAX_DIRS {
        let remaining = report.large_untracked_dirs.len() - MAX_DIRS;
        parts.push(format!("{remaining} more"));
    }
    Some(format!(
        "Repository snapshot ignored large untracked directories (>= {threshold} files): {}. These directories are excluded from snapshots and undo cleanup. Adjust `ghost_snapshot.ignore_large_untracked_dirs` to change this behavior.",
        parts.join(", ")
    ))
}

fn format_ignored_untracked_files_warning(
    ignore_large_untracked_files: Option<i64>,
    report: &GhostSnapshotReport,
) -> Option<String> {
    let threshold = ignore_large_untracked_files?;
    if report.ignored_untracked_files.is_empty() {
        return None;
    }

    const MAX_FILES: usize = 3;
    let mut parts: Vec<String> = Vec::new();
    for file in report.ignored_untracked_files.iter().take(MAX_FILES) {
        parts.push(format!(
            "{} ({})",
            file.path.display(),
            format_bytes(file.byte_size)
        ));
    }
    if report.ignored_untracked_files.len() > MAX_FILES {
        let remaining = report.ignored_untracked_files.len() - MAX_FILES;
        parts.push(format!("{remaining} more"));
    }

    Some(format!(
        "Repository snapshot ignored untracked files larger than {}: {}. These files are preserved during undo cleanup, but their contents are not captured in the snapshot. Adjust `ghost_snapshot.ignore_large_untracked_files` to change this behavior. To avoid this message in the future, update your `.gitignore`.",
        format_bytes(threshold),
        parts.join(", ")
    ))
}

fn format_bytes(bytes: i64) -> String {
    const KIB: i64 = 1024;
    const MIB: i64 = 1024 * 1024;

    if bytes >= MIB {
        return format!("{} MiB", bytes / MIB);
    }
    if bytes >= KIB {
        return format!("{} KiB", bytes / KIB);
    }
    format!("{bytes} B")
}
