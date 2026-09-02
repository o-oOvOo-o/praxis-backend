use std::path::Path;

use praxis_protocol::parse_command::ParsedCommand;

use super::shlex_join;

const CONNECTORS: &[&str] = &["&&", "||", "|", ";"];

pub(super) fn normalize_top_level(command: &[String]) -> Vec<String> {
    match command {
        [answer, pipe, rest @ ..]
            if matches!(answer.as_str(), "yes" | "y" | "no" | "n") && pipe == "|" =>
        {
            rest.to_vec()
        }
        [shell, flag, script]
            if matches!(shell.as_str(), "bash" | "zsh")
                && matches!(flag.as_str(), "-c" | "-lc") =>
        {
            shlex::split(script).unwrap_or_else(|| command.to_vec())
        }
        _ => command.to_vec(),
    }
}

pub(super) fn split_segments(tokens: &[String]) -> Vec<Vec<String>> {
    let mut segments = Vec::new();
    let mut start = 0;
    for (index, token) in tokens.iter().enumerate() {
        if CONNECTORS.contains(&token.as_str()) {
            if start < index {
                segments.push(tokens[start..index].to_vec());
            }
            start = index + 1;
        }
    }
    if start < tokens.len() {
        segments.push(tokens[start..].to_vec());
    }
    if segments.is_empty() && tokens.is_empty() {
        segments.push(Vec::new());
    }
    segments
}

pub(super) fn trim_at_connector(tokens: &[String]) -> Vec<String> {
    tokens
        .iter()
        .take_while(|token| !CONNECTORS.contains(&token.as_str()))
        .cloned()
        .collect()
}

pub(super) fn short_display_path(path: &str) -> String {
    let normalized = path.replace('\\', "/");
    let trimmed = normalized.trim_end_matches('/');
    trimmed
        .rsplit('/')
        .find(|part| {
            !part.is_empty() && !matches!(*part, "build" | "dist" | "node_modules" | "src")
        })
        .unwrap_or(trimmed)
        .to_owned()
}

pub(super) fn skip_flag_values<'a>(
    arguments: &'a [String],
    value_options: &[&str],
) -> Vec<&'a String> {
    let mut retained = Vec::new();
    let mut index = 0;
    while index < arguments.len() {
        let argument = &arguments[index];
        if argument == "--" {
            retained.extend(arguments[index + 1..].iter());
            break;
        }
        if argument.starts_with("--") && argument.contains('=') {
            index += 1;
            continue;
        }
        if value_options.contains(&argument.as_str()) {
            index += 2;
            continue;
        }
        retained.push(argument);
        index += 1;
    }
    retained
}

pub(super) fn positional_operands<'a>(
    arguments: &'a [String],
    value_options: &[&str],
) -> Vec<&'a String> {
    skip_flag_values(arguments, value_options)
        .into_iter()
        .filter(|argument| !argument.starts_with('-'))
        .collect()
}

pub(super) fn first_non_flag_operand(
    arguments: &[String],
    value_options: &[&str],
) -> Option<String> {
    positional_operands(arguments, value_options)
        .first()
        .cloned()
        .cloned()
}

pub(super) fn single_non_flag_operand(
    arguments: &[String],
    value_options: &[&str],
) -> Option<String> {
    let operands = positional_operands(arguments, value_options);
    (operands.len() == 1).then(|| operands[0].clone())
}

pub(crate) fn is_valid_sed_n_arg(argument: Option<&str>) -> bool {
    let Some(range) = argument.and_then(|value| value.strip_suffix('p')) else {
        return false;
    };
    let valid_number =
        |number: &str| !number.is_empty() && number.bytes().all(|byte| byte.is_ascii_digit());
    match range.split_once(',') {
        Some((start, end)) => valid_number(start) && valid_number(end) && !end.contains(','),
        None => valid_number(range),
    }
}

pub(super) fn sed_read_path(arguments: &[String]) -> Option<String> {
    let arguments = trim_at_connector(arguments);
    if !arguments.iter().any(|argument| argument == "-n") {
        return None;
    }
    let has_range = arguments.iter().enumerate().any(|(index, argument)| {
        is_valid_sed_n_arg(Some(argument))
            && (index == 0 || !matches!(arguments[index - 1].as_str(), "-f" | "--file"))
    });
    if !has_range {
        return None;
    }
    let operands: Vec<String> =
        skip_flag_values(&arguments, &["-e", "-f", "--expression", "--file"])
            .into_iter()
            .filter(|argument| !argument.starts_with('-'))
            .cloned()
            .collect();
    operands
        .iter()
        .position(|operand| is_valid_sed_n_arg(Some(operand)))
        .and_then(|index| operands.get(index + 1).cloned())
        .or_else(|| operands.first().cloned())
}

pub(super) fn parse_grep_like(command: &[String], arguments: &[String]) -> ParsedCommand {
    let arguments = trim_at_connector(arguments);
    let mut explicit_pattern = None;
    let mut operands = Vec::new();
    let mut index = 0;
    let mut positional = false;
    while index < arguments.len() {
        let argument = &arguments[index];
        if argument == "--" {
            positional = true;
            index += 1;
            continue;
        }
        if !positional {
            match argument.as_str() {
                "-e" | "--regexp" | "-f" | "--file" => {
                    if explicit_pattern.is_none() {
                        explicit_pattern = arguments.get(index + 1).cloned();
                    }
                    index += 2;
                    continue;
                }
                "-m" | "--max-count" | "-C" | "--context" | "-A" | "--after-context" | "-B"
                | "--before-context" => {
                    index += 2;
                    continue;
                }
                _ if argument.starts_with('-') => {
                    index += 1;
                    continue;
                }
                _ => {}
            }
        }
        operands.push(argument.clone());
        index += 1;
    }
    let explicit = explicit_pattern.is_some();
    let query = explicit_pattern.or_else(|| operands.first().cloned());
    let path = operands
        .get(if explicit { 0 } else { 1 })
        .map(|value| short_display_path(value));
    ParsedCommand::Search {
        cmd: shlex_join(command),
        query,
        path,
    }
}

pub(super) fn awk_data_file_operand(arguments: &[String]) -> Option<String> {
    let arguments = trim_at_connector(arguments);
    let script_from_file = arguments
        .iter()
        .any(|argument| matches!(argument.as_str(), "-f" | "--file"));
    let operands = positional_operands(
        &arguments,
        &["-F", "-v", "-f", "--field-separator", "--assign", "--file"],
    );
    operands
        .get(if script_from_file { 0 } else { 1 })
        .cloned()
        .cloned()
}

pub(super) fn python_walks_files(arguments: &[String]) -> bool {
    let script = arguments
        .windows(2)
        .find(|pair| pair[0] == "-c")
        .map(|pair| pair[1].as_str());
    script.is_some_and(|script| {
        [
            "os.walk",
            "os.listdir",
            "os.scandir",
            "glob.glob",
            "glob.iglob",
            "pathlib.Path",
            ".rglob(",
        ]
        .iter()
        .any(|needle| script.contains(needle))
    })
}

pub(super) fn is_python_command(program: &str) -> bool {
    matches!(program, "python" | "python2" | "python3")
        || program.starts_with("python2.")
        || program.starts_with("python3.")
}

pub(super) fn cd_target(arguments: &[String]) -> Option<String> {
    if let Some(index) = arguments.iter().position(|argument| argument == "--") {
        return arguments.get(index + 1).cloned();
    }
    arguments
        .iter()
        .filter(|argument| !argument.starts_with('-'))
        .next_back()
        .cloned()
}

pub(super) fn parse_fd_query_and_path(arguments: &[String]) -> (Option<String>, Option<String>) {
    let arguments = trim_at_connector(arguments);
    let operands = positional_operands(
        &arguments,
        &[
            "-t",
            "--type",
            "-e",
            "--extension",
            "-E",
            "--exclude",
            "--search-path",
        ],
    );
    match operands.as_slice() {
        [only] if path_like(only) => (None, Some(short_display_path(only))),
        [only] => (Some((*only).clone()), None),
        [query, path, ..] => (Some((*query).clone()), Some(short_display_path(path))),
        [] => (None, None),
    }
}

pub(super) fn parse_find_query_and_path(arguments: &[String]) -> (Option<String>, Option<String>) {
    let arguments = trim_at_connector(arguments);
    let path = arguments
        .iter()
        .find(|argument| {
            !argument.starts_with('-') && !matches!(argument.as_str(), "!" | "(" | ")")
        })
        .map(|value| short_display_path(value));
    let query = arguments.windows(2).find_map(|pair| {
        matches!(pair[0].as_str(), "-name" | "-iname" | "-path" | "-regex").then(|| pair[1].clone())
    });
    (query, path)
}

fn path_like(value: &str) -> bool {
    matches!(value, "." | "..")
        || value.starts_with("./")
        || value.starts_with("../")
        || value.contains('/')
        || value.contains('\\')
}

pub(super) fn join_paths(base: &str, relative: &str) -> String {
    if absolute_like(relative) || base.is_empty() {
        return relative.to_owned();
    }
    Path::new(base)
        .join(relative)
        .to_string_lossy()
        .into_owned()
}

fn absolute_like(path: &str) -> bool {
    Path::new(path).is_absolute()
        || path
            .as_bytes()
            .get(..3)
            .is_some_and(|prefix| prefix[0].is_ascii_alphabetic() && &prefix[1..] == b":\\")
        || path.starts_with("\\\\")
}
