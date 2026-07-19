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
