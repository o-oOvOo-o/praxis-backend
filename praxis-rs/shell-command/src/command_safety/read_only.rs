use super::git::find_subcommand;
use super::git::has_prompting_global;
use super::identity::executable_name;
use crate::parse_command::is_valid_sed_n_arg;

const INTRINSIC_READERS: &[&str] = &[
    "cat", "cd", "cut", "echo", "expr", "false", "grep", "head", "id", "ls", "nl", "paste", "pwd",
    "rev", "seq", "stat", "tail", "tr", "true", "uname", "uniq", "wc", "which", "whoami",
];
const FIND_EFFECTS: &[&str] = &[
    "-exec", "-execdir", "-ok", "-okdir", "-delete", "-fls", "-fprint", "-fprint0", "-fprintf",
];
const GIT_EFFECTS: &[&str] = &[
    "--output",
    "--ext-diff",
    "--textconv",
    "--exec",
    "--paginate",
];

pub(super) fn accepts(command: &[String]) -> bool {
    let Some(name) = command.first().and_then(|value| executable_name(value)) else {
        return false;
    };
    match name.as_ref() {
        name if INTRINSIC_READERS.contains(&name) => true,
        "numfmt" | "tac" if cfg!(target_os = "linux") => true,
        "base64" => !arguments(command).any(base64_writes),
        "find" => !command
            .iter()
            .any(|value| FIND_EFFECTS.contains(&value.as_str())),
        "rg" => !arguments(command).any(ripgrep_executes),
        "git" => git_is_read_only(command),
        "sed" => sed_is_read_only(command),
        _ => false,
    }
}

fn arguments(command: &[String]) -> impl Iterator<Item = &str> {
    command.iter().skip(1).map(String::as_str)
}

fn base64_writes(argument: &str) -> bool {
    argument.starts_with("-o") || argument == "--output" || argument.starts_with("--output=")
}

fn ripgrep_executes(argument: &str) -> bool {
    matches!(argument, "--search-zip" | "-z")
        || option_with_value(argument, "--pre")
        || option_with_value(argument, "--hostname-bin")
}

fn option_with_value(argument: &str, option: &str) -> bool {
    argument == option
        || argument
            .strip_prefix(option)
            .is_some_and(|suffix| suffix.starts_with('='))
}

fn git_is_read_only(command: &[String]) -> bool {
    if has_prompting_global(command) {
        return false;
    }
    let Some(subcommand) = find_subcommand(
        command,
        &["status", "log", "diff", "show", "branch", "cat-file"],
    ) else {
        return false;
    };
    let arguments = &command[subcommand.index + 1..];
    git_arguments_are_read_only(arguments)
        && (subcommand.name != "branch" || branch_is_read_only(arguments))
}

fn git_arguments_are_read_only(arguments: &[String]) -> bool {
    !arguments.iter().any(|argument| {
        GIT_EFFECTS.contains(&argument.as_str())
            || option_with_value(argument, "--output")
            || option_with_value(argument, "--exec")
    })
}

fn branch_is_read_only(arguments: &[String]) -> bool {
    arguments.is_empty()
        || arguments.iter().all(|argument| {
            matches!(
                argument.as_str(),
                "--list"
                    | "-l"
                    | "--show-current"
                    | "-a"
                    | "--all"
                    | "-r"
                    | "--remotes"
                    | "-v"
                    | "-vv"
                    | "--verbose"
            ) || argument.starts_with("--format=")
        })
}

fn sed_is_read_only(command: &[String]) -> bool {
    command.len() <= 4
        && command.get(1).is_some_and(|argument| argument == "-n")
        && is_valid_sed_n_arg(command.get(2).map(String::as_str))
}
