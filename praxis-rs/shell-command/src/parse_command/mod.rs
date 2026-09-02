use std::path::PathBuf;

use praxis_protocol::parse_command::ParsedCommand;
use shlex::split as shlex_split;
use shlex::try_join;

use crate::bash::extract_bash_command;
use crate::powershell::extract_powershell_command;

mod arguments;
mod shell_plan;
mod summary;

pub(crate) use arguments::is_valid_sed_n_arg;
use arguments::*;
use summary::summarize_main_tokens;

pub fn shlex_join(tokens: &[String]) -> String {
    try_join(tokens.iter().map(String::as_str))
        .unwrap_or_else(|_| "<command included NUL byte>".to_owned())
}

pub fn extract_shell_command(command: &[String]) -> Option<(&str, &str)> {
    extract_bash_command(command).or_else(|| extract_powershell_command(command))
}

/// Projects an argv vector into stable, human-readable command metadata.
pub fn parse_command(command: &[String]) -> Vec<ParsedCommand> {
    let projected = parse_command_impl(command);
    let coalesced = projected.into_iter().fold(Vec::new(), |mut output, item| {
        if output.last() != Some(&item) {
            output.push(item);
        }
        output
    });
    if coalesced.iter().any(is_unknown) {
        vec![unknown_for_original(command)]
    } else {
        coalesced
    }
}

pub fn parse_command_impl(command: &[String]) -> Vec<ParsedCommand> {
    if let Some(projected) = shell_plan::project(command) {
        return projected;
    }
    if let Some((_, script)) = extract_powershell_command(command) {
        return vec![ParsedCommand::Unknown {
            cmd: script.to_owned(),
        }];
    }

    let normalized = normalize_top_level(command);
    let segments = split_segments(&normalized);
    let mut projector = CommandProjector::default();
    for segment in segments {
        projector.accept(&segment);
    }
    clean_projection(projector.finish())
}

fn unknown_for_original(command: &[String]) -> ParsedCommand {
    let rendered = extract_shell_command(command)
        .map(|(_, script)| script.to_owned())
        .unwrap_or_else(|| shlex_join(command));
    ParsedCommand::Unknown { cmd: rendered }
}

fn is_unknown(command: &ParsedCommand) -> bool {
    matches!(command, ParsedCommand::Unknown { .. })
}

#[derive(Default)]
struct CommandProjector {
    working_directory: Option<String>,
    output: Vec<ParsedCommand>,
}

impl CommandProjector {
    fn accept(&mut self, tokens: &[String]) {
        if let Some(("cd", arguments)) = head_and_tail(tokens) {
            if let Some(next) = cd_target(arguments) {
                self.working_directory = Some(match self.working_directory.as_deref() {
                    Some(current) => join_paths(current, &next),
                    None => next,
                });
            }
            return;
        }

        let projected = summarize_main_tokens(tokens);
        self.output.push(self.rebase_read(projected));
    }

    fn rebase_read(&self, projected: ParsedCommand) -> ParsedCommand {
        match (projected, self.working_directory.as_deref()) {
            (ParsedCommand::Read { cmd, name, path }, Some(base)) => ParsedCommand::Read {
                cmd,
                name,
                path: PathBuf::from(join_paths(base, &path.to_string_lossy())),
            },
            (other, _) => other,
        }
    }

    fn finish(self) -> Vec<ParsedCommand> {
        self.output
    }
}

fn head_and_tail(tokens: &[String]) -> Option<(&str, &[String])> {
    tokens
        .split_first()
        .map(|(head, tail)| (head.as_str(), tail))
}

fn clean_projection(commands: Vec<ParsedCommand>) -> Vec<ParsedCommand> {
    if commands.len() < 2 {
        return commands;
    }
    commands
        .into_iter()
        .enumerate()
        .filter_map(|(index, command)| {
            let removable = match &command {
                ParsedCommand::Unknown { cmd } => {
                    let words = shlex_split(cmd).unwrap_or_default();
                    let program = words.first().map(String::as_str);
                    cmd == "true"
                        || (index == 0 && program == Some("echo"))
                        || (program == Some("cd"))
                        || (program == Some("nl")
                            && words.iter().skip(1).all(|word| word.starts_with('-')))
                }
                _ => false,
            };
            (!removable).then_some(command)
        })
        .collect()
}

#[cfg(test)]
#[path = "../parse_command_tests.rs"]
mod tests;
