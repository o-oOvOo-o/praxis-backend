use super::identity::executable_name;

const GLOBAL_VALUES: &[&str] = &[
    "-C",
    "-c",
    "--config-env",
    "--exec-path",
    "--git-dir",
    "--namespace",
    "--super-prefix",
    "--work-tree",
];
const PROMPTING_GLOBALS: &[&str] = &[
    "-c",
    "--config",
    "--config-env",
    "--exec-path",
    "--git-dir",
    "--namespace",
    "--super-prefix",
    "--work-tree",
];

pub(super) struct Subcommand<'command> {
    pub(super) index: usize,
    pub(super) name: &'command str,
}

pub(super) fn find_subcommand<'command>(
    command: &'command [String],
    accepted: &[&str],
) -> Option<Subcommand<'command>> {
    (executable_name(command.first()?)?.as_ref() == "git").then_some(())?;
    let mut arguments = command.iter().enumerate().skip(1);
    while let Some((index, argument)) = arguments.next() {
        if has_inline_value(argument) {
            continue;
        }
        if GLOBAL_VALUES.contains(&argument.as_str()) {
            arguments.next()?;
            continue;
        }
        if argument == "--" || argument.starts_with('-') {
            continue;
        }
        return accepted.contains(&argument.as_str()).then_some(Subcommand {
            index,
            name: argument,
        });
    }
    None
}

pub(super) fn has_prompting_global(command: &[String]) -> bool {
    command
        .iter()
        .skip(1)
        .any(|argument| option_requires_prompt(argument))
}

pub(super) fn option_requires_prompt(argument: &str) -> bool {
    PROMPTING_GLOBALS.contains(&argument) || prompting_inline_value(argument).is_some()
}

fn has_inline_value(argument: &str) -> bool {
    prompting_inline_value(argument).is_some()
        || argument
            .strip_prefix("-C")
            .is_some_and(|value| !value.is_empty())
}

fn prompting_inline_value(argument: &str) -> Option<&str> {
    if let Some(value) = argument.strip_prefix("-c")
        && !value.is_empty()
    {
        return Some(value);
    }
    PROMPTING_GLOBALS
        .iter()
        .filter(|option| option.starts_with("--"))
        .find_map(|option| argument.strip_prefix(option)?.strip_prefix('='))
}
