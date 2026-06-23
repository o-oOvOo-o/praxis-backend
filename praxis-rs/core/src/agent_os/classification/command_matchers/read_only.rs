pub(in crate::agent_os::classification) fn is_read_only_command(command: &str) -> bool {
    [
        "rg ",
        "grep ",
        "get-content",
        "select-string",
        "ls",
        "dir",
        "git status",
        "git diff",
        "git show",
        "git log",
        "findstr",
        "type ",
        "cat ",
    ]
    .iter()
    .any(|needle| command.contains(needle))
}
