use super::*;

pub(super) fn dirty_violation_path(
    dirty_files: &[PathBuf],
    task: &TaskRecord,
    profile: Option<&CapabilityProfile>,
) -> Option<PathBuf> {
    dirty_files
        .iter()
        .find(|path| {
            !dirty_file_allowed_by_task(task, path)
                || !profile.is_some_and(|profile| profile.path_scopes.allows(path))
        })
        .cloned()
}
