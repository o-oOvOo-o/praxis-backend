use std::path::Path;
use std::path::PathBuf;
use std::process::Command;

use praxis_utils_absolute_path::AbsolutePathBuf;

use crate::shell_detect::ShellType;
use crate::shell_detect::detect_shell_type;

/// Script prefix that makes captured PowerShell output UTF-8.
pub const UTF8_OUTPUT_PREFIX: &str = "[Console]::OutputEncoding=[System.Text.Encoding]::UTF8;\n";

struct Invocation<'a> {
    executable: &'a str,
    script: &'a str,
    script_index: usize,
}

impl<'a> Invocation<'a> {
    fn parse(argv: &'a [String]) -> Option<Self> {
        let executable = argv.first()?;
        if detect_shell_type(Path::new(executable)) != Some(ShellType::PowerShell) {
            return None;
        }
        for (offset, argument) in argv[1..].iter().enumerate() {
            match argument.to_ascii_lowercase().as_str() {
                "-nologo" | "-noprofile" => {}
                "-command" | "-c" => {
                    let script_index = offset + 2;
                    return Some(Self {
                        executable,
                        script: argv.get(script_index)?,
                        script_index,
                    });
                }
                _ => return None,
            }
        }
        None
    }
}

/// Adds UTF-8 output configuration to a recognized PowerShell invocation.
pub fn prefix_powershell_script_with_utf8(command: &[String]) -> Vec<String> {
    let Some(invocation) = Invocation::parse(command) else {
        return command.to_vec();
    };
    if invocation
        .script
        .trim_start()
        .starts_with(UTF8_OUTPUT_PREFIX)
    {
        return command.to_vec();
    }
    let mut prefixed = command.to_vec();
    prefixed[invocation.script_index] = format!("{UTF8_OUTPUT_PREFIX}{}", invocation.script);
    prefixed
}

/// Extracts the executable and script from a supported PowerShell invocation.
pub fn extract_powershell_command(command: &[String]) -> Option<(&str, &str)> {
    let invocation = Invocation::parse(command)?;
    Some((invocation.executable, invocation.script))
}

struct ExecutableLocator;

impl ExecutableLocator {
    fn from_path(name: &str) -> Option<AbsolutePathBuf> {
        Self::accept(which::which(name).ok()?)
    }

    fn accept(path: PathBuf) -> Option<AbsolutePathBuf> {
        if !probe(&path) {
            return None;
        }
        AbsolutePathBuf::from_absolute_path(path).ok()
    }

    fn pwsh_home() -> Option<AbsolutePathBuf> {
        let output = Command::new("cmd")
            .args(["/C", "pwsh", "-NoProfile", "-Command", "$PSHOME"])
            .output()
            .ok()?;
        let home = output
            .status
            .success()
            .then(|| String::from_utf8_lossy(&output.stdout))?;
        let home = home.trim();
        if home.is_empty() {
            return None;
        }
        Self::accept(Path::new(home).join("pwsh.exe"))
    }
}

/// Locates a working Windows PowerShell executable on `PATH`.
pub fn try_find_powershell_executable_blocking() -> Option<AbsolutePathBuf> {
    ExecutableLocator::from_path("powershell.exe")
}

/// Locates a working PowerShell Core executable through its home or `PATH`.
pub fn try_find_pwsh_executable_blocking() -> Option<AbsolutePathBuf> {
    ExecutableLocator::pwsh_home().or_else(|| ExecutableLocator::from_path("pwsh.exe"))
}

fn probe(executable: &Path) -> bool {
    Command::new(executable)
        .args(["-NoLogo", "-NoProfile", "-Command", "Write-Output ok"])
        .output()
        .is_ok_and(|output| output.status.success())
}

#[cfg(test)]
mod tests {
    use super::UTF8_OUTPUT_PREFIX;
    use super::extract_powershell_command;
    use super::prefix_powershell_script_with_utf8;

    #[test]
    fn extracts_basic_powershell_command() {
        let cmd = vec![
            "powershell".to_string(),
            "-Command".to_string(),
            "Write-Host hi".to_string(),
        ];
        let (_shell, script) = extract_powershell_command(&cmd).expect("extract");
        assert_eq!(script, "Write-Host hi");
    }

    #[test]
    fn extracts_lowercase_flags() {
        let cmd = vec![
            "powershell".to_string(),
            "-nologo".to_string(),
            "-command".to_string(),
            "Write-Host hi".to_string(),
        ];
        let (_shell, script) = extract_powershell_command(&cmd).expect("extract");
        assert_eq!(script, "Write-Host hi");
    }

    #[test]
    fn extracts_full_path_powershell_command() {
        let command = if cfg!(windows) {
            "C:\\windows\\System32\\WindowsPowerShell\\v1.0\\powershell.exe".to_string()
        } else {
            "/usr/local/bin/powershell.exe".to_string()
        };
        let cmd = vec![command, "-Command".to_string(), "Write-Host hi".to_string()];
        let (_shell, script) = extract_powershell_command(&cmd).expect("extract");
        assert_eq!(script, "Write-Host hi");
    }

    #[test]
    fn extracts_with_noprofile_and_alias() {
        let cmd = vec![
            "pwsh".to_string(),
            "-NoProfile".to_string(),
            "-c".to_string(),
            "Get-ChildItem | Select-String foo".to_string(),
        ];
        let (_shell, script) = extract_powershell_command(&cmd).expect("extract");
        assert_eq!(script, "Get-ChildItem | Select-String foo");
    }

    #[test]
    fn utf8_prefix_is_inserted_exactly_once_after_leading_whitespace() {
        let command = vec![
            "pwsh".to_string(),
            "-Command".to_string(),
            format!("  {UTF8_OUTPUT_PREFIX}Get-ChildItem"),
        ];
        assert_eq!(prefix_powershell_script_with_utf8(&command), command);
    }

    #[test]
    fn non_powershell_invocations_are_untouched() {
        let command = vec!["bash".to_string(), "-c".to_string(), "echo ok".to_string()];
        assert_eq!(prefix_powershell_script_with_utf8(&command), command);
    }
}
