use std::path::Path;

use crate::agent_os::model::ScopedPaths;
use crate::path_scope::normalize_path_for_scope;
use crate::path_scope::scope_matches;

impl ScopedPaths {
    pub(in crate::agent_os) fn allows(&self, path: &Path) -> bool {
        let value = normalize_path_for_scope(path);
        if self
            .deny
            .iter()
            .any(|pattern| scope_matches(pattern, &value))
        {
            return false;
        }
        self.allow.is_empty()
            || self
                .allow
                .iter()
                .any(|pattern| scope_matches(pattern, &value))
    }
}
