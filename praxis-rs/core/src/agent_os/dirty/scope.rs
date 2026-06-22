use std::path::Path;

use crate::agent_os::model::TaskRecord;
use crate::path_scope::normalize_path_for_scope;
use crate::path_scope::scope_matches;

pub(in crate::agent_os) fn dirty_file_allowed_by_task(task: &TaskRecord, path: &Path) -> bool {
    if task.exploratory || task.scope.is_empty() {
        return true;
    }
    let value = normalize_path_for_scope(path);
    task.scope
        .iter()
        .any(|pattern| scope_matches(pattern, &value))
}
