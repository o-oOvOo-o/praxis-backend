use std::path::PathBuf;

pub(in crate::agent_os) fn format_dirty_file_report(
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
