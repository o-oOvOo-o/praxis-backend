use shlex::split as split_shell_words;
use url::Url;

use crate::command_safety::identity::executable_name;
use crate::command_safety::powershell_invocation::PowerShellInvocation;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowsCommandRisk {
    ExternalNavigation,
    ForcedDeletion,
    RecursiveDeletion,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowsCommandSurface {
    PowerShell,
    CommandPrompt,
    DirectGuiLaunch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowsCommandAssessment {
    pub surface: WindowsCommandSurface,
    pub risk: WindowsCommandRisk,
    pub evidence: &'static str,
}

#[derive(Clone, Copy)]
struct Finding {
    risk: WindowsCommandRisk,
    evidence: &'static str,
}

impl Finding {
    const fn new(risk: WindowsCommandRisk, evidence: &'static str) -> Self {
        Self { risk, evidence }
    }

    const fn on(self, surface: WindowsCommandSurface) -> WindowsCommandAssessment {
        WindowsCommandAssessment {
            surface,
            risk: self.risk,
            evidence: self.evidence,
        }
    }
}

pub fn assess_windows_command(command: &[String]) -> Option<WindowsCommandAssessment> {
    powershell_risk(command)
        .map(|finding| finding.on(WindowsCommandSurface::PowerShell))
        .or_else(|| {
            command_prompt_risk(command)
                .map(|finding| finding.on(WindowsCommandSurface::CommandPrompt))
        })
        .or_else(|| {
            direct_launch_risk(command)
                .map(|finding| finding.on(WindowsCommandSurface::DirectGuiLaunch))
        })
}

pub fn is_dangerous_command_windows(command: &[String]) -> bool {
    assess_windows_command(command).is_some()
}

fn powershell_risk(command: &[String]) -> Option<Finding> {
    let invocation = PowerShellInvocation::parse(command)?;
    let source = invocation.source()?;
    let tokens = split_shell_words(&source)?;
    let normalized: Vec<String> = tokens.iter().map(|token| normalize(token)).collect();
    let has_url = tokens.iter().any(|token| contains_http_url(token));

    if has_url
        && normalized.iter().any(|token| {
            matches!(
                token.as_str(),
                "start-process" | "start" | "saps" | "invoke-item" | "ii"
            ) || token.contains("start-process")
                || token.contains("invoke-item")
        })
    {
        return Some(Finding::new(
            WindowsCommandRisk::ExternalNavigation,
            "PowerShell launches an external URL",
        ));
    }
    if has_url
        && normalized
            .iter()
            .any(|token| token.contains("shellexecute") || token.contains("shell.application"))
    {
        return Some(Finding::new(
            WindowsCommandRisk::ExternalNavigation,
            "PowerShell invokes ShellExecute for an external URL",
        ));
    }
    if has_url {
        match normalized.first().map(String::as_str) {
            Some("rundll32")
                if normalized
                    .iter()
                    .any(|token| token.contains("url.dll,fileprotocolhandler")) =>
            {
                return Some(Finding::new(
                    WindowsCommandRisk::ExternalNavigation,
                    "rundll32 dispatches a URL protocol handler",
                ));
            }
            Some("mshta") => {
                return Some(Finding::new(
                    WindowsCommandRisk::ExternalNavigation,
                    "mshta opens an external URL",
                ));
            }
            Some(name) if is_browser(name) => {
                return Some(Finding::new(
                    WindowsCommandRisk::ExternalNavigation,
                    "browser process opens an external URL",
                ));
            }
            Some("explorer" | "explorer.exe") => {
                return Some(Finding::new(
                    WindowsCommandRisk::ExternalNavigation,
                    "Explorer opens an external URL",
                ));
            }
            _ => {}
        }
    }
    force_delete_in_same_segment(&normalized).then_some(Finding::new(
        WindowsCommandRisk::ForcedDeletion,
        "PowerShell deletion uses -Force",
    ))
}

fn command_prompt_risk(command: &[String]) -> Option<Finding> {
    let (executable, arguments) = command.split_first()?;
    (executable_name(executable)?.as_ref() == "cmd").then_some(())?;
    let body = cmd_body(arguments)?;
    let tokens = if let [single] = body {
        split_shell_words(single).unwrap_or_else(|| vec![single.to_owned()])
    } else {
        body.to_vec()
    };
    let tokens: Vec<String> = tokens
        .iter()
        .flat_map(|token| split_cmd_operators(token))
        .collect();
    for segment in tokens.split(|token| matches!(token.as_str(), "&" | "&&" | "|" | "||")) {
        let Some(command) = segment.first() else {
            continue;
        };
        if command.eq_ignore_ascii_case("start")
            && segment.iter().any(|token| contains_http_url(token))
        {
            return Some(Finding::new(
                WindowsCommandRisk::ExternalNavigation,
                "cmd start opens an external URL",
            ));
        }
        if matches_ignore_ascii_case(command, &["del", "erase"]) && has_flag(segment, "/f") {
            return Some(Finding::new(
                WindowsCommandRisk::ForcedDeletion,
                "cmd deletion uses /f",
            ));
        }
        if matches_ignore_ascii_case(command, &["rd", "rmdir"])
            && has_flag(segment, "/s")
            && has_flag(segment, "/q")
        {
            return Some(Finding::new(
                WindowsCommandRisk::RecursiveDeletion,
                "cmd directory removal combines /s and /q",
            ));
        }
    }
    None
}

fn cmd_body(arguments: &[String]) -> Option<&[String]> {
    for (index, argument) in arguments.iter().enumerate() {
        match argument.to_ascii_lowercase().as_str() {
            "/c" | "/r" | "-c" => {
                return arguments.get(index + 1..).filter(|body| !body.is_empty());
            }
            option if option.starts_with('/') => {}
            _ => return None,
        }
    }
    None
}

fn direct_launch_risk(command: &[String]) -> Option<Finding> {
    let (executable, arguments) = command.split_first()?;
    let name = executable_name(executable)?;
    let has_url = arguments.iter().any(|argument| contains_http_url(argument));
    if !has_url {
        return None;
    }
    match name.as_ref() {
        "explorer" => Some(Finding::new(
            WindowsCommandRisk::ExternalNavigation,
            "Explorer opens an external URL",
        )),
        "mshta" => Some(Finding::new(
            WindowsCommandRisk::ExternalNavigation,
            "mshta opens an external URL",
        )),
        "rundll32"
            if arguments.iter().any(|argument| {
                argument
                    .to_ascii_lowercase()
                    .contains("url.dll,fileprotocolhandler")
            }) =>
        {
            Some(Finding::new(
                WindowsCommandRisk::ExternalNavigation,
                "rundll32 dispatches a URL protocol handler",
            ))
        }
        name if is_browser(name) => Some(Finding::new(
            WindowsCommandRisk::ExternalNavigation,
            "browser process opens an external URL",
        )),
        _ => None,
    }
}

fn split_cmd_operators(token: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut start = 0;
    let mut characters = token.char_indices().peekable();
    while let Some((index, character)) = characters.next() {
        if !matches!(character, '&' | '|') {
            continue;
        }
        push_trimmed(&mut parts, &token[start..index]);
        let end = if let Some((next, repeated)) = characters.next_if(|(_, next)| *next == character)
        {
            next + repeated.len_utf8()
        } else {
            index + character.len_utf8()
        };
        parts.push(token[index..end].to_owned());
        start = end;
    }
    push_trimmed(&mut parts, &token[start..]);
    parts
}

fn push_trimmed(parts: &mut Vec<String>, value: &str) {
    let value = value.trim();
    if !value.is_empty() {
        parts.push(value.to_owned());
    }
}

fn force_delete_in_same_segment(tokens: &[String]) -> bool {
    const DELETE_COMMANDS: &[&str] = &["remove-item", "ri", "rm", "del", "erase", "rd", "rmdir"];
    let source = tokens.join(" ");
    source
        .split(|character| matches!(character, ';' | '|' | '&' | '\n' | '\r' | '\t'))
        .any(|segment| {
            let atoms: Vec<&str> = segment
                .split(|character: char| {
                    character.is_whitespace()
                        || matches!(character, '{' | '}' | '(' | ')' | '[' | ']' | ',')
                })
                .filter(|atom| !atom.is_empty())
                .collect();
            atoms.iter().any(|atom| DELETE_COMMANDS.contains(atom))
                && atoms.iter().any(|atom| {
                    atom.eq_ignore_ascii_case("-force")
                        || atom
                            .get(..7)
                            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("-force:"))
                })
        })
}

fn has_flag(arguments: &[String], flag: &str) -> bool {
    arguments
        .iter()
        .any(|argument| argument.eq_ignore_ascii_case(flag))
}

fn matches_ignore_ascii_case(value: &str, candidates: &[&str]) -> bool {
    candidates
        .iter()
        .any(|candidate| value.eq_ignore_ascii_case(candidate))
}

fn contains_http_url(token: &str) -> bool {
    let lower = token.to_ascii_lowercase();
    let Some(index) = lower.find("https://").or_else(|| lower.find("http://")) else {
        return false;
    };
    let candidate = token[index..].trim_end_matches(|character: char| {
        character.is_whitespace() || matches!(character, '\'' | '"' | ';' | ')' | ']' | '}' | ',')
    });
    Url::parse(candidate).is_ok_and(|url| matches!(url.scheme(), "http" | "https"))
}

fn normalize(token: &str) -> String {
    token
        .trim_matches(|character: char| {
            character.is_whitespace() || matches!(character, '\'' | '"' | '(' | ')')
        })
        .to_ascii_lowercase()
}

fn is_browser(name: &str) -> bool {
    matches!(
        name,
        "chrome"
            | "chrome.exe"
            | "msedge"
            | "msedge.exe"
            | "firefox"
            | "firefox.exe"
            | "iexplore"
            | "iexplore.exe"
    )
}

#[cfg(test)]
mod tests {
    use super::WindowsCommandRisk;
    use super::assess_windows_command;
    use super::is_dangerous_command_windows;

    fn vec_str(items: &[&str]) -> Vec<String> {
        items.iter().map(std::string::ToString::to_string).collect()
    }

    #[test]
    fn powershell_start_process_url_is_dangerous() {
        let command = vec_str(&[
            "powershell",
            "-NoLogo",
            "-Command",
            "Start-Process 'https://example.com'",
        ]);
        assert!(is_dangerous_command_windows(&command));
        let assessment = assess_windows_command(&command).expect("risk assessment");
        assert_eq!(assessment.risk, WindowsCommandRisk::ExternalNavigation);
        assert!(assessment.evidence.contains("external URL"));
    }

    #[test]
    fn powershell_start_process_url_with_trailing_semicolon_is_dangerous() {
        assert!(is_dangerous_command_windows(&vec_str(&[
            "powershell",
            "-Command",
            "Start-Process('https://example.com');"
        ])));
    }

    #[test]
    fn powershell_start_process_local_is_not_flagged() {
        assert!(!is_dangerous_command_windows(&vec_str(&[
            "powershell",
            "-Command",
            "Start-Process notepad.exe"
        ])));
    }

    #[test]
    fn cmd_start_with_url_is_dangerous() {
        assert!(is_dangerous_command_windows(&vec_str(&[
            "cmd",
            "/c",
            "start",
            "https://example.com"
        ])));
    }

    #[test]
    fn msedge_with_url_is_dangerous() {
        assert!(is_dangerous_command_windows(&vec_str(&[
            "msedge.exe",
            "https://example.com"
        ])));
    }

    #[test]
    fn explorer_with_directory_is_not_flagged() {
        assert!(!is_dangerous_command_windows(&vec_str(&[
            "explorer.exe",
            "."
        ])));
    }

    // Force delete tests for PowerShell

    #[test]
    fn powershell_remove_item_force_is_dangerous() {
        assert!(is_dangerous_command_windows(&vec_str(&[
            "powershell",
            "-Command",
            "Remove-Item test -Force"
        ])));
    }

    #[test]
    fn powershell_remove_item_recurse_force_is_dangerous() {
        assert!(is_dangerous_command_windows(&vec_str(&[
            "powershell",
            "-Command",
            "Remove-Item test -Recurse -Force"
        ])));
    }

    #[test]
    fn powershell_ri_alias_force_is_dangerous() {
        assert!(is_dangerous_command_windows(&vec_str(&[
            "pwsh",
            "-Command",
            "ri test -Force"
        ])));
    }

    #[test]
    fn powershell_remove_item_without_force_is_not_flagged() {
        assert!(!is_dangerous_command_windows(&vec_str(&[
            "powershell",
            "-Command",
            "Remove-Item test"
        ])));
    }

    // Force delete tests for CMD
    #[test]
    fn cmd_del_force_is_dangerous() {
        assert!(is_dangerous_command_windows(&vec_str(&[
            "cmd", "/c", "del", "/f", "test.txt"
        ])));
    }

    #[test]
    fn cmd_erase_force_is_dangerous() {
        assert!(is_dangerous_command_windows(&vec_str(&[
            "cmd", "/c", "erase", "/f", "test.txt"
        ])));
    }

    #[test]
    fn cmd_del_without_force_is_not_flagged() {
        assert!(!is_dangerous_command_windows(&vec_str(&[
            "cmd", "/c", "del", "test.txt"
        ])));
    }

    #[test]
    fn cmd_rd_recursive_is_dangerous() {
        assert!(is_dangerous_command_windows(&vec_str(&[
            "cmd", "/c", "rd", "/s", "/q", "test"
        ])));
    }

    #[test]
    fn cmd_rd_without_quiet_is_not_flagged() {
        assert!(!is_dangerous_command_windows(&vec_str(&[
            "cmd", "/c", "rd", "/s", "test"
        ])));
    }

    #[test]
    fn cmd_rmdir_recursive_is_dangerous() {
        assert!(is_dangerous_command_windows(&vec_str(&[
            "cmd", "/c", "rmdir", "/s", "/q", "test"
        ])));
    }

    // Test exact scenario from issue #8567
    #[test]
    fn powershell_remove_item_path_recurse_force_is_dangerous() {
        assert!(is_dangerous_command_windows(&vec_str(&[
            "powershell",
            "-Command",
            "Remove-Item -Path 'test' -Recurse -Force"
        ])));
    }

    #[test]
    fn powershell_remove_item_force_with_semicolon_is_dangerous() {
        assert!(is_dangerous_command_windows(&vec_str(&[
            "powershell",
            "-Command",
            "Remove-Item test -Force; Write-Host done"
        ])));
    }

    #[test]
    fn powershell_remove_item_force_inside_block_is_dangerous() {
        assert!(is_dangerous_command_windows(&vec_str(&[
            "powershell",
            "-Command",
            "if ($true) { Remove-Item test -Force}"
        ])));
    }

    #[test]
    fn powershell_remove_item_force_inside_brackets_is_dangerous() {
        assert!(is_dangerous_command_windows(&vec_str(&[
            "powershell",
            "-Command",
            "[void]( Remove-Item test -Force)]"
        ])));
    }

    #[test]
    fn cmd_del_path_containing_f_is_not_flagged() {
        assert!(!is_dangerous_command_windows(&vec_str(&[
            "cmd",
            "/c",
            "del",
            "C:/foo/bar.txt"
        ])));
    }

    #[test]
    fn cmd_rd_path_containing_s_is_not_flagged() {
        assert!(!is_dangerous_command_windows(&vec_str(&[
            "cmd",
            "/c",
            "rd",
            "C:/source"
        ])));
    }

    #[test]
    fn cmd_bypass_chained_del_is_dangerous() {
        assert!(is_dangerous_command_windows(&vec_str(&[
            "cmd", "/c", "echo", "hello", "&", "del", "/f", "file.txt"
        ])));
    }

    #[test]
    fn powershell_chained_no_space_is_dangerous() {
        assert!(is_dangerous_command_windows(&vec_str(&[
            "powershell",
            "-Command",
            "Write-Host hi;Remove-Item -Force C:\\tmp"
        ])));
    }

    #[test]
    fn powershell_comma_separated_is_dangerous() {
        assert!(is_dangerous_command_windows(&vec_str(&[
            "powershell",
            "-Command",
            "del,-Force,C:\\foo"
        ])));
    }

    #[test]
    fn cmd_echo_del_is_not_dangerous() {
        assert!(!is_dangerous_command_windows(&vec_str(&[
            "cmd", "/c", "echo", "del", "/f"
        ])));
    }

    #[test]
    fn cmd_del_single_string_argument_is_dangerous() {
        assert!(is_dangerous_command_windows(&vec_str(&[
            "cmd",
            "/c",
            "del /f file.txt"
        ])));
    }

    #[test]
    fn cmd_del_chained_single_string_argument_is_dangerous() {
        assert!(is_dangerous_command_windows(&vec_str(&[
            "cmd",
            "/c",
            "echo hello & del /f file.txt"
        ])));
    }

    #[test]
    fn cmd_chained_no_space_del_is_dangerous() {
        assert!(is_dangerous_command_windows(&vec_str(&[
            "cmd",
            "/c",
            "echo hi&del /f file.txt"
        ])));
    }

    #[test]
    fn cmd_chained_andand_no_space_del_is_dangerous() {
        assert!(is_dangerous_command_windows(&vec_str(&[
            "cmd",
            "/c",
            "echo hi&&del /f file.txt"
        ])));
    }

    #[test]
    fn cmd_chained_oror_no_space_del_is_dangerous() {
        assert!(is_dangerous_command_windows(&vec_str(&[
            "cmd",
            "/c",
            "echo hi||del /f file.txt"
        ])));
    }

    #[test]
    fn cmd_start_url_single_string_is_dangerous() {
        assert!(is_dangerous_command_windows(&vec_str(&[
            "cmd",
            "/c",
            "start https://example.com"
        ])));
    }

    #[test]
    fn cmd_chained_no_space_rmdir_is_dangerous() {
        assert!(is_dangerous_command_windows(&vec_str(&[
            "cmd",
            "/c",
            "echo hi&rmdir /s /q testdir"
        ])));
    }

    #[test]
    fn cmd_del_force_uppercase_flag_is_dangerous() {
        assert!(is_dangerous_command_windows(&vec_str(&[
            "cmd", "/c", "DEL", "/F", "file.txt"
        ])));
    }

    #[test]
    fn cmdexe_r_del_force_is_dangerous() {
        assert!(is_dangerous_command_windows(&vec_str(&[
            "cmd.exe", "/r", "del", "/f", "file.txt"
        ])));
    }

    #[test]
    fn cmd_start_quoted_url_single_string_is_dangerous() {
        assert!(is_dangerous_command_windows(&vec_str(&[
            "cmd",
            "/c",
            r#"start "https://example.com""#
        ])));
    }

    #[test]
    fn cmd_start_title_then_url_is_dangerous() {
        assert!(is_dangerous_command_windows(&vec_str(&[
            "cmd",
            "/c",
            r#"start "" https://example.com"#
        ])));
    }

    #[test]
    fn powershell_rm_alias_force_is_dangerous() {
        assert!(is_dangerous_command_windows(&vec_str(&[
            "powershell",
            "-Command",
            "rm test -Force"
        ])));
    }

    #[test]
    fn powershell_benign_force_separate_command_is_not_dangerous() {
        assert!(!is_dangerous_command_windows(&vec_str(&[
            "powershell",
            "-Command",
            "Get-ChildItem -Force; Remove-Item test"
        ])));
    }
}
