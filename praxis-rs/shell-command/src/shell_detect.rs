use std::path::Path;

use serde::Deserialize;
use serde::Serialize;

#[derive(Debug, PartialEq, Eq, Clone, Copy, Serialize, Deserialize)]
pub enum ShellType {
    Zsh,
    Bash,
    PowerShell,
    Sh,
    Cmd,
}

/// Identifies a supported shell from an executable name or path.
pub fn detect_shell_type(shell_path: &Path) -> Option<ShellType> {
    let name = shell_path.file_stem()?.to_str()?;
    if name.eq_ignore_ascii_case("zsh") {
        Some(ShellType::Zsh)
    } else if name.eq_ignore_ascii_case("bash") {
        Some(ShellType::Bash)
    } else if name.eq_ignore_ascii_case("pwsh") || name.eq_ignore_ascii_case("powershell") {
        Some(ShellType::PowerShell)
    } else if name.eq_ignore_ascii_case("sh") {
        Some(ShellType::Sh)
    } else if name.eq_ignore_ascii_case("cmd") {
        Some(ShellType::Cmd)
    } else {
        None
    }
}

impl ShellType {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Zsh => "zsh",
            Self::Bash => "bash",
            Self::PowerShell => "powershell",
            Self::Sh => "sh",
            Self::Cmd => "cmd",
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::ShellType;
    use super::detect_shell_type;

    #[test]
    fn executable_identity_is_path_and_ascii_case_independent() {
        assert_eq!(
            detect_shell_type(&PathBuf::from(r"C:\Tools\PWSH.EXE")),
            Some(ShellType::PowerShell)
        );
        assert_eq!(
            detect_shell_type(&PathBuf::from("/usr/local/bin/BASH")),
            Some(ShellType::Bash)
        );
    }
}
