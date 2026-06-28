use super::model::ToolGroupKind;

pub(crate) fn inspection_group_for_command(command: &str) -> ToolGroupKind {
    let lower = command.to_ascii_lowercase();
    if lower.contains(" && ")
        || lower.contains(" ; ")
        || lower.contains('\n')
        || lower.contains("\r\n")
    {
        ToolGroupKind::Mixed
    } else if lower.contains("apply_patch") || lower.contains("git apply") {
        ToolGroupKind::Patch
    } else if lower.contains("mcp__") || lower.contains("list_mcp") {
        ToolGroupKind::Mcp
    } else if lower.contains("web.run")
        || lower.contains("search_query")
        || lower.contains("image_query")
    {
        ToolGroupKind::WebSearch
    } else if lower.contains("multi_tool_use")
        || lower.contains("spawn")
        || lower.contains("agent")
        || lower.contains("worker")
    {
        ToolGroupKind::Worker
    } else if lower.contains("rg ")
        || lower.starts_with("rg ")
        || lower.contains("ripgrep ")
        || lower.contains("select-string ")
        || lower.contains("findstr ")
    {
        ToolGroupKind::Search
    } else if lower.contains("get-childitem")
        || lower.contains("list_directory")
        || lower.contains("list_dir")
        || lower.contains(" list_directory")
        || lower.contains("list-directory")
        || lower.contains(" dir ")
        || lower.starts_with("dir ")
        || lower.contains(" ls ")
        || lower.starts_with("ls ")
    {
        ToolGroupKind::Directory
    } else if lower.contains("get-content ")
        || lower.contains(" cat ")
        || lower.starts_with("cat ")
        || lower.contains(" type ")
        || lower.starts_with("type ")
        || lower.contains("select-object ")
    {
        ToolGroupKind::FileRead
    } else {
        ToolGroupKind::Shell
    }
}
