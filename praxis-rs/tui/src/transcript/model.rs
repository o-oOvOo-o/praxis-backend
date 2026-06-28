#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ToolGroupKind {
    Shell,
    Search,
    Directory,
    FileRead,
    Patch,
    Mcp,
    WebSearch,
    Worker,
    Mixed,
}
