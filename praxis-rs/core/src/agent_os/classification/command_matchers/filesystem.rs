pub(in crate::agent_os::classification) fn is_file_write_command(command: &str) -> bool {
    [
        "apply_patch",
        "set-content",
        "out-file",
        "new-item",
        "remove-item",
        "move-item",
        "copy-item",
        "python -c",
        "node -e",
        "tee ",
    ]
    .iter()
    .any(|needle| command.contains(needle))
}

pub(in crate::agent_os::classification) fn has_file_redirection(command: &str) -> bool {
    let bytes = command.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != b'>' {
            index += 1;
            continue;
        }
        let mut cursor = index + 1;
        if cursor < bytes.len() && bytes[cursor] == b'>' {
            cursor += 1;
        }
        if cursor < bytes.len() && bytes[cursor] == b'&' {
            index = cursor + 1;
            continue;
        }
        return true;
    }
    false
}

pub(in crate::agent_os::classification) fn is_git_mutation(command: &str) -> bool {
    [
        "git commit",
        "git rebase",
        "git merge",
        "git checkout",
        "git switch",
        "git reset",
        "git clean",
        "git stash",
        "git add",
        "git rm",
        "git mv",
    ]
    .iter()
    .any(|needle| command.contains(needle))
}
