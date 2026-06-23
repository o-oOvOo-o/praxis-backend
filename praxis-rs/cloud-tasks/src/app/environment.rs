#[derive(Clone, Debug, Default)]
pub struct EnvironmentRow {
    pub id: String,
    pub label: Option<String>,
    pub is_pinned: bool,
    pub repo_hints: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct EnvModalState {
    pub query: String,
    pub selected: usize,
}
