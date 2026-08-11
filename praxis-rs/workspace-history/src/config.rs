use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceHistoryConfig {
    pub max_store_bytes: u64,
    pub retention_days: u32,
    pub max_file_bytes: u64,
    pub ignored_directory_names: HashSet<String>,
}

impl Default for WorkspaceHistoryConfig {
    fn default() -> Self {
        Self {
            max_store_bytes: 10 * 1024 * 1024 * 1024,
            retention_days: 90,
            max_file_bytes: 64 * 1024 * 1024,
            ignored_directory_names: [
                ".git",
                ".praxis",
                ".local",
                "target",
                "node_modules",
                ".venv",
                "venv",
                "dist",
                "build",
                ".cache",
                "__pycache__",
            ]
            .into_iter()
            .map(str::to_owned)
            .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn machine_local_workspace_data_is_ignored_by_default() {
        assert!(
            WorkspaceHistoryConfig::default()
                .ignored_directory_names
                .contains(".local")
        );
    }
}
