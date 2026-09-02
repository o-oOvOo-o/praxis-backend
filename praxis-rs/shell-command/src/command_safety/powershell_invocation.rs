use std::borrow::Cow;
use std::path::Path;

use super::powershell_parser::PowershellParseOutcome;
use super::powershell_parser::parse_with_powershell_ast;
use crate::shell_detect::ShellType;
use crate::shell_detect::detect_shell_type;

const SETUP_SWITCHES: &[&str] = &["-nologo", "-noprofile", "-noninteractive", "-mta", "-sta"];
const OPAQUE_SWITCHES: &[&str] = &[
    "-encodedcommand",
    "-ec",
    "-file",
    "/file",
    "-windowstyle",
    "-executionpolicy",
    "-workingdirectory",
];

pub(super) struct PowerShellInvocation<'a> {
    executable: &'a str,
    arguments: &'a [String],
}

impl<'a> PowerShellInvocation<'a> {
    pub(super) fn parse(command: &'a [String]) -> Option<Self> {
        let (executable, arguments) = command.split_first()?;
        (detect_shell_type(Path::new(executable)) == Some(ShellType::PowerShell)).then_some(Self {
            executable,
            arguments,
        })
    }

    pub(super) fn commands(&self) -> Option<Vec<Vec<String>>> {
        let source = self.source()?;
        match parse_with_powershell_ast(self.executable, &source) {
            PowershellParseOutcome::Commands(commands) if !commands.is_empty() => Some(commands),
            _ => None,
        }
    }

    pub(super) fn source(&self) -> Option<Cow<'a, str>> {
        let mut index = 0;
        while let Some(argument) = self.arguments.get(index) {
            let switch = argument.to_ascii_lowercase();
            match switch.as_str() {
                "-command" | "/command" | "-c" => {
                    let script = self.arguments.get(index + 1)?;
                    (index + 2 == self.arguments.len()).then_some(())?;
                    return Some(Cow::Borrowed(script));
                }
                value if value.starts_with("-command:") || value.starts_with("/command:") => {
                    (index + 1 == self.arguments.len()).then_some(())?;
                    return argument
                        .split_once(':')
                        .map(|(_, script)| Cow::Borrowed(script));
                }
                value if SETUP_SWITCHES.contains(&value) => index += 1,
                value
                    if OPAQUE_SWITCHES.contains(&value)
                        || value.starts_with('-')
                        || value.starts_with('/') =>
                {
                    return None;
                }
                _ => return Some(Cow::Owned(join_arguments(&self.arguments[index..]))),
            }
        }
        None
    }
}

fn join_arguments(arguments: &[String]) -> String {
    let Some((command, rest)) = arguments.split_first() else {
        return String::new();
    };
    std::iter::once(command.to_owned())
        .chain(rest.iter().map(|argument| quote_argument(argument)))
        .collect::<Vec<_>>()
        .join(" ")
}

fn quote_argument(argument: &str) -> String {
    if !argument.is_empty()
        && argument.chars().all(|character| {
            character.is_ascii_alphanumeric()
                || matches!(
                    character,
                    '_' | '.' | '/' | '\\' | ':' | '-' | '?' | '*' | '=' | ','
                )
        })
    {
        return argument.to_owned();
    }
    format!("'{}'", argument.replace('\'', "''"))
}
