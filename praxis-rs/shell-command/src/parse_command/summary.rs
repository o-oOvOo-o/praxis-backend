use std::path::PathBuf;

use praxis_protocol::parse_command::ParsedCommand;

use super::arguments::*;
use super::shlex_join;

struct OperandSpec {
    names: &'static [&'static str],
    value_options: &'static [&'static str],
}

const LIST_SPECS: &[OperandSpec] = &[
    OperandSpec {
        names: &["ls"],
        value_options: &[
            "-I",
            "-w",
            "--block-size",
            "--format",
            "--time-style",
            "--color",
            "--quoting-style",
        ],
    },
    OperandSpec {
        names: &["eza", "exa"],
        value_options: &[
            "-I",
            "--ignore-glob",
            "--color",
            "--sort",
            "--time-style",
            "--time",
        ],
    },
    OperandSpec {
        names: &["tree"],
        value_options: &["-L", "-P", "-I", "--charset", "--filelimit", "--sort"],
    },
    OperandSpec {
        names: &["du"],
        value_options: &[
            "-d",
            "--max-depth",
            "-B",
            "--block-size",
            "--exclude",
            "--time-style",
        ],
    },
];

const READ_SPECS: &[OperandSpec] = &[
    OperandSpec {
        names: &["cat", "more"],
        value_options: &[],
    },
    OperandSpec {
        names: &["bat", "batcat"],
        value_options: &[
            "--theme",
            "--language",
            "--style",
            "--terminal-width",
            "--tabs",
            "--line-range",
            "--map-syntax",
        ],
    },
    OperandSpec {
        names: &["less"],
        value_options: &[
            "-p",
            "-P",
            "-x",
            "-y",
            "-z",
            "-j",
            "--pattern",
            "--prompt",
            "--tabs",
            "--shift",
            "--jump-target",
        ],
    },
];

pub(super) fn summarize_main_tokens(tokens: &[String]) -> ParsedCommand {
    let Some((program, arguments)) = tokens.split_first() else {
        return unknown(tokens);
    };

    if let Some(spec) = find_spec(LIST_SPECS, program) {
        return list_files(
            tokens,
            first_non_flag_operand(arguments, spec.value_options),
        );
    }
    if let Some(spec) = find_spec(READ_SPECS, program) {
        return single_non_flag_operand(arguments, spec.value_options)
            .map(|path| read_file(tokens, path))
            .unwrap_or_else(|| unknown(tokens));
    }

    match program.as_str() {
        "rg" | "rga" | "ripgrep-all" => summarize_ripgrep(tokens, arguments),
        "git" => summarize_git(tokens, arguments),
        "fd" => summarize_fd(tokens, arguments),
        "find" => summarize_find(tokens, arguments),
        "grep" | "egrep" | "fgrep" => parse_grep_like(tokens, arguments),
        "ag" | "ack" | "pt" => summarize_search_alias(tokens, arguments),
        "head" => summarize_counted_reader(tokens, arguments, false),
        "tail" => summarize_counted_reader(tokens, arguments, true),
        "awk" => awk_data_file_operand(arguments)
            .map(|path| read_file(tokens, path))
            .unwrap_or_else(|| unknown(tokens)),
        "nl" => summarize_numbered_lines(tokens, arguments),
        "sed" => sed_read_path(arguments)
            .map(|path| read_file(tokens, path))
            .unwrap_or_else(|| unknown(tokens)),
        executable if is_python_command(executable) && python_walks_files(arguments) => {
            list_files(tokens, None)
        }
        _ => unknown(tokens),
    }
}

fn find_spec<'a>(specs: &'a [OperandSpec], program: &str) -> Option<&'a OperandSpec> {
    specs.iter().find(|spec| spec.names.contains(&program))
}

fn rendered(tokens: &[String]) -> String {
    shlex_join(tokens)
}

fn unknown(tokens: &[String]) -> ParsedCommand {
    ParsedCommand::Unknown {
        cmd: rendered(tokens),
    }
}

fn list_files(tokens: &[String], path: Option<String>) -> ParsedCommand {
    ParsedCommand::ListFiles {
        cmd: rendered(tokens),
        path: path.map(|value| short_display_path(&value)),
    }
}

fn read_file(tokens: &[String], path: String) -> ParsedCommand {
    ParsedCommand::Read {
        cmd: rendered(tokens),
        name: short_display_path(&path),
        path: PathBuf::from(path),
    }
}

fn search(tokens: &[String], query: Option<String>, path: Option<String>) -> ParsedCommand {
    ParsedCommand::Search {
        cmd: rendered(tokens),
        query,
        path: path.map(|value| short_display_path(&value)),
    }
}

fn summarize_ripgrep(tokens: &[String], arguments: &[String]) -> ParsedCommand {
    const VALUE_OPTIONS: &[&str] = &[
        "-g",
        "--glob",
        "--iglob",
        "-t",
        "--type",
        "--type-add",
        "--type-not",
        "-m",
        "--max-count",
        "-A",
        "-B",
        "-C",
        "--context",
        "--max-depth",
    ];
    let arguments = trim_at_connector(arguments);
    let lists_only = arguments.iter().any(|argument| argument == "--files");
    let operands = non_flag_operands(&arguments, VALUE_OPTIONS);
    if lists_only {
        return list_files(tokens, operands.first().cloned().cloned());
    }
    search(
        tokens,
        operands.first().cloned().cloned(),
        operands.get(1).cloned().cloned(),
    )
}

fn summarize_git(tokens: &[String], arguments: &[String]) -> ParsedCommand {
    let Some((operation, tail)) = arguments.split_first() else {
        return unknown(tokens);
    };
    match operation.as_str() {
        "grep" => parse_grep_like(tokens, tail),
        "ls-files" => list_files(
            tokens,
            first_non_flag_operand(
                tail,
                &["--exclude", "--exclude-from", "--pathspec-from-file"],
            ),
        ),
        _ => unknown(tokens),
    }
}

fn summarize_fd(tokens: &[String], arguments: &[String]) -> ParsedCommand {
    let (query, path) = parse_fd_query_and_path(arguments);
    match query {
        Some(query) => search(tokens, Some(query), path),
        None => list_files(tokens, path),
    }
}

fn summarize_find(tokens: &[String], arguments: &[String]) -> ParsedCommand {
    let (query, path) = parse_find_query_and_path(arguments);
    match query {
        Some(query) => search(tokens, Some(query), path),
        None => list_files(tokens, path),
    }
}

fn summarize_search_alias(tokens: &[String], arguments: &[String]) -> ParsedCommand {
    const VALUE_OPTIONS: &[&str] = &[
        "-G",
        "-g",
        "--file-search-regex",
        "--ignore-dir",
        "--ignore-file",
        "--path-to-ignore",
    ];
    let arguments = trim_at_connector(arguments);
    let operands = non_flag_operands(&arguments, VALUE_OPTIONS);
    search(
        tokens,
        operands.first().cloned().cloned(),
        operands.get(1).cloned().cloned(),
    )
}

fn non_flag_operands<'a>(arguments: &'a [String], value_options: &[&str]) -> Vec<&'a String> {
    skip_flag_values(arguments, value_options)
        .into_iter()
        .filter(|argument| !argument.starts_with('-'))
        .collect()
}

fn summarize_counted_reader(
    tokens: &[String],
    arguments: &[String],
    permits_positive_offset: bool,
) -> ParsedCommand {
    if let [path] = arguments
        && !path.starts_with('-')
    {
        return read_file(tokens, path.clone());
    }

    let Some((count, remaining)) = count_option(arguments) else {
        return unknown(tokens);
    };
    if !valid_count(count, permits_positive_offset) {
        return unknown(tokens);
    }
    remaining
        .iter()
        .find(|argument| !argument.starts_with('-'))
        .cloned()
        .map(|path| read_file(tokens, path))
        .unwrap_or_else(|| unknown(tokens))
}

fn count_option(arguments: &[String]) -> Option<(&str, &[String])> {
    match arguments {
        [flag, count, remaining @ ..] if flag == "-n" => Some((count, remaining)),
        [joined, remaining @ ..] if joined.starts_with("-n") => Some((&joined[2..], remaining)),
        _ => None,
    }
}

fn valid_count(value: &str, permits_positive_offset: bool) -> bool {
    let digits = if permits_positive_offset {
        value.strip_prefix('+').unwrap_or(value)
    } else {
        value
    };
    !digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit())
}

fn summarize_numbered_lines(tokens: &[String], arguments: &[String]) -> ParsedCommand {
    const VALUE_OPTIONS: &[&str] = &["-s", "-w", "-v", "-i", "-b"];
    non_flag_operands(arguments, VALUE_OPTIONS)
        .first()
        .cloned()
        .cloned()
        .map(|path| read_file(tokens, path))
        .unwrap_or_else(|| unknown(tokens))
}
