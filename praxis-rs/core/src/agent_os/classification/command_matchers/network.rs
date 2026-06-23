pub(in crate::agent_os::classification) fn is_network_command(command: &str) -> bool {
    [
        "curl ",
        "wget ",
        "git clone",
        "npm install",
        "pnpm install",
        "yarn install",
        "cargo fetch",
        "pip install",
        "uv pip install",
    ]
    .iter()
    .any(|needle| command.contains(needle))
}
