use praxis_protocol::parse_command::ParsedCommand;

use crate::bash::extract_bash_command;
use crate::bash::try_parse_shell;
use crate::bash::try_parse_word_only_commands_sequence;

use super::CommandProjector;
use super::arguments::awk_data_file_operand;
use super::arguments::sed_read_path;
use super::clean_projection;
use super::shlex_join;

pub(super) fn project(original: &[String]) -> Option<Vec<ParsedCommand>> {
    let (_, script) = extract_bash_command(original)?;
    let Some(tree) = try_parse_shell(script) else {
        return Some(vec![unknown(script)]);
    };
    let Some(commands) = try_parse_word_only_commands_sequence(&tree, script) else {
        return Some(vec![unknown(script)]);
    };
    if commands.is_empty() {
        return Some(vec![unknown(script)]);
    }

    let multiple_commands = commands.len() > 1;
    let primary: Vec<Vec<String>> = commands
        .into_iter()
        .filter(|command| !formatting_only(command))
        .collect();
    if primary.is_empty() {
        return Some(vec![unknown(script)]);
    }

    let mut projector = CommandProjector::default();
    for command in primary {
        projector.accept(&command);
    }
    let projected = clean_projection(projector.finish());
    Some(rewrite_single_display(projected, script, multiple_commands))
}

fn unknown(script: &str) -> ParsedCommand {
    ParsedCommand::Unknown {
        cmd: script.to_owned(),
    }
}

fn rewrite_single_display(
    mut projected: Vec<ParsedCommand>,
    script: &str,
    multiple_commands: bool,
) -> Vec<ParsedCommand> {
    if projected.len() != 1 {
        return projected;
    }
    let words = shlex::split(script).unwrap_or_else(|| vec![script.to_owned()]);
    let has_connector = multiple_commands
        || words
            .iter()
            .any(|word| matches!(word.as_str(), "|" | "&&" | "||" | ";"));
    let whole_command = shlex_join(&words);
    let Some(item) = projected.pop() else {
        return projected;
    };
    let rewritten = match item {
        ParsedCommand::Read { cmd, name, path } => {
            let piped_sed = words.iter().any(|word| word == "|")
                && words
                    .windows(2)
                    .any(|pair| pair[0] == "sed" && pair[1] == "-n");
            ParsedCommand::Read {
                cmd: if piped_sed {
                    script.to_owned()
                } else if has_connector {
                    cmd
                } else {
                    whole_command
                },
                name,
                path,
            }
        }
        ParsedCommand::ListFiles { cmd, path } => ParsedCommand::ListFiles {
            cmd: if has_connector { cmd } else { whole_command },
            path,
        },
        ParsedCommand::Search { cmd, query, path } => ParsedCommand::Search {
            cmd: if has_connector { cmd } else { whole_command },
            query,
            path,
        },
        other => other,
    };
    vec![rewritten]
}

fn formatting_only(tokens: &[String]) -> bool {
    let Some((program, arguments)) = tokens.split_first() else {
        return false;
    };
    match program.as_str() {
        "wc" | "tr" | "cut" | "sort" | "uniq" | "tee" | "column" | "yes" | "printf" => true,
        "xargs" => !xargs_mutates(arguments),
        "awk" => awk_data_file_operand(arguments).is_none(),
        "head" => count_filter_without_file(arguments, false),
        "tail" => count_filter_without_file(arguments, true),
        "sed" => sed_read_path(arguments).is_none(),
        _ => false,
    }
}

fn count_filter_without_file(arguments: &[String], allow_positive: bool) -> bool {
    match arguments {
        [] => true,
        [only] => only.starts_with('-'),
        [flag, count] if matches!(flag.as_str(), "-n" | "-c") => {
            let digits = if allow_positive {
                count.strip_prefix('+').unwrap_or(count)
            } else {
                count
            };
            !digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit())
        }
        _ => false,
    }
}

fn xargs_mutates(arguments: &[String]) -> bool {
    let Some(command) = xargs_command(arguments) else {
        return false;
    };
    let Some((program, options)) = command.split_first() else {
        return false;
    };
    match program.as_str() {
        "perl" | "ruby" => has_in_place(options),
        "sed" => has_in_place(options) || options.iter().any(|option| option == "--in-place"),
        "rg" => options.iter().any(|option| option == "--replace"),
        _ => false,
    }
}

fn xargs_command(arguments: &[String]) -> Option<&[String]> {
    let mut index = 0;
    while let Some(argument) = arguments.get(index) {
        if argument == "--" {
            return arguments.get(index + 1..).filter(|tail| !tail.is_empty());
        }
        if !argument.starts_with('-') {
            return arguments.get(index..).filter(|tail| !tail.is_empty());
        }
        let separate_value = argument.len() == 2
            && matches!(
                argument.as_str(),
                "-E" | "-e" | "-I" | "-L" | "-n" | "-P" | "-s"
            );
        index += if separate_value { 2 } else { 1 };
    }
    None
}

fn has_in_place(options: &[String]) -> bool {
    options.iter().any(|option| {
        matches!(option.as_str(), "-i" | "-pi")
            || option.starts_with("-i")
            || option.starts_with("-pi")
    })
}
