use crate::command_safety::powershell_invocation::PowerShellInvocation;
use crate::command_safety::read_only;

const POWERSHELL_READERS: &[&str] = &[
    "echo",
    "write-output",
    "write-host",
    "dir",
    "ls",
    "get-childitem",
    "gci",
    "cat",
    "type",
    "gc",
    "get-content",
    "select-string",
    "sls",
    "findstr",
    "measure-object",
    "measure",
    "get-location",
    "gl",
    "pwd",
    "test-path",
    "tp",
    "resolve-path",
    "rvpa",
    "select-object",
    "select",
    "get-item",
];
const POWERSHELL_MUTATORS: &[&str] = &[
    "set-content",
    "add-content",
    "out-file",
    "new-item",
    "remove-item",
    "move-item",
    "copy-item",
    "rename-item",
    "start-process",
    "stop-process",
];

/// Accepts only PowerShell invocations whose complete AST projection is read-only.
pub fn is_safe_command_windows(command: &[String]) -> bool {
    PowerShellInvocation::parse(command)
        .and_then(|invocation| invocation.commands())
        .is_some_and(|commands| commands.iter().all(|words| command_is_read_only(words)))
}

fn command_is_read_only(words: &[String]) -> bool {
    let Some(primary) = words.first().map(|word| normalized_token(word)) else {
        return false;
    };
    if words
        .iter()
        .map(|word| normalized_token(word))
        .any(|word| POWERSHELL_MUTATORS.contains(&word.as_str()))
    {
        return false;
    }
    POWERSHELL_READERS.contains(&primary.as_str())
        || matches!(primary.as_str(), "git" | "rg") && read_only::accepts(words)
}

fn normalized_token(word: &str) -> String {
    word.trim_matches(|character| matches!(character, '(' | ')'))
        .trim_start_matches('-')
        .to_ascii_lowercase()
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use crate::powershell::try_find_pwsh_executable_blocking;
    use std::string::ToString;

    /// Converts a slice of string literals into owned `String`s for the tests.
    fn vec_str(args: &[&str]) -> Vec<String> {
        args.iter().map(ToString::to_string).collect()
    }

    #[test]
    fn recognizes_safe_powershell_wrappers() {
        assert!(is_safe_command_windows(&vec_str(&[
            "powershell.exe",
            "-NoLogo",
            "-Command",
            "Get-ChildItem -Path .",
        ])));

        assert!(is_safe_command_windows(&vec_str(&[
            "powershell.exe",
            "-NoProfile",
            "-Command",
            "git status",
        ])));

        assert!(is_safe_command_windows(&vec_str(&[
            "powershell.exe",
            "Get-Content",
            "Cargo.toml",
        ])));

        // pwsh parity
        if let Some(pwsh) = try_find_pwsh_executable_blocking() {
            assert!(is_safe_command_windows(&[
                pwsh.as_path().to_str().unwrap().into(),
                "-NoProfile".to_string(),
                "-Command".to_string(),
                "Get-ChildItem".to_string(),
            ]));
        }
    }

    #[test]
    fn accepts_full_path_powershell_invocations() {
        if !cfg!(windows) {
            // Windows only because on Linux path splitting doesn't handle `/` separators properly
            return;
        }

        if let Some(pwsh) = try_find_pwsh_executable_blocking() {
            assert!(is_safe_command_windows(&[
                pwsh.as_path().to_str().unwrap().into(),
                "-NoProfile".to_string(),
                "-Command".to_string(),
                "Get-ChildItem -Path .".to_string(),
            ]));
        }

        assert!(is_safe_command_windows(&vec_str(&[
            r"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe",
            "-Command",
            "Get-Content Cargo.toml",
        ])));
    }

    #[test]
    fn allows_read_only_pipelines_and_git_usage() {
        let Some(pwsh) = try_find_pwsh_executable_blocking() else {
            return;
        };

        let pwsh: String = pwsh.as_path().to_str().unwrap().into();
        assert!(is_safe_command_windows(&[
            pwsh.clone(),
            "-NoLogo".to_string(),
            "-NoProfile".to_string(),
            "-Command".to_string(),
            "rg --files-with-matches foo | Measure-Object | Select-Object -ExpandProperty Count"
                .to_string()
        ]));

        assert!(is_safe_command_windows(&[
            pwsh.clone(),
            "-NoLogo".to_string(),
            "-NoProfile".to_string(),
            "-Command".to_string(),
            "Get-Content foo.rs | Select-Object -Skip 200".to_string()
        ]));

        assert!(is_safe_command_windows(&[
            pwsh.clone(),
            "-Command".to_string(),
            "-git cat-file -p HEAD:foo.rs".to_string()
        ]));

        assert!(is_safe_command_windows(&[
            pwsh.clone(),
            "-Command".to_string(),
            "(Get-Content foo.rs -Raw)".to_string()
        ]));

        assert!(is_safe_command_windows(&[
            pwsh,
            "-Command".to_string(),
            "Get-Item foo.rs | Select-Object Length".to_string()
        ]));
    }

    #[test]
    fn rejects_git_global_override_options() {
        let Some(pwsh) = try_find_pwsh_executable_blocking() else {
            return;
        };

        let pwsh: String = pwsh.as_path().to_str().unwrap().into();
        for script in [
            "git -c core.pager=cat show HEAD:foo.rs",
            "git --config-env core.pager=PAGER show HEAD:foo.rs",
            "git --config-env=core.pager=PAGER show HEAD:foo.rs",
            "git --git-dir .evil-git diff HEAD~1..HEAD",
            "git --git-dir=.evil-git diff HEAD~1..HEAD",
            "git --work-tree . status",
            "git --work-tree=. status",
            "git --exec-path .git/helpers show HEAD:foo.rs",
            "git --exec-path=.git/helpers show HEAD:foo.rs",
            "git --namespace attacker show HEAD:foo.rs",
            "git --namespace=attacker show HEAD:foo.rs",
            "git --super-prefix attacker/ show HEAD:foo.rs",
            "git --super-prefix=attacker/ show HEAD:foo.rs",
        ] {
            assert!(
                !is_safe_command_windows(&[
                    pwsh.clone(),
                    "-NoLogo".to_string(),
                    "-NoProfile".to_string(),
                    "-Command".to_string(),
                    script.to_string(),
                ]),
                "expected {script:?} to require approval due to unsafe git global option",
            );
        }
    }

    #[test]
    fn rejects_powershell_commands_with_side_effects() {
        assert!(!is_safe_command_windows(&vec_str(&[
            "powershell.exe",
            "-NoLogo",
            "-Command",
            "Remove-Item foo.txt",
        ])));

        assert!(!is_safe_command_windows(&vec_str(&[
            "powershell.exe",
            "-NoProfile",
            "-Command",
            "rg --pre cat",
        ])));

        assert!(!is_safe_command_windows(&vec_str(&[
            "powershell.exe",
            "-Command",
            "Set-Content foo.txt 'hello'",
        ])));

        // Redirections are blocked
        assert!(!is_safe_command_windows(&vec_str(&[
            "powershell.exe",
            "-Command",
            "echo hi > out.txt",
        ])));
        assert!(!is_safe_command_windows(&vec_str(&[
            "powershell.exe",
            "-Command",
            "Get-Content x | Out-File y",
        ])));
        assert!(!is_safe_command_windows(&vec_str(&[
            "powershell.exe",
            "-Command",
            "Write-Output foo 2> err.txt",
        ])));

        // Call operator is blocked
        assert!(!is_safe_command_windows(&vec_str(&[
            "powershell.exe",
            "-Command",
            "& Remove-Item foo",
        ])));

        // Chained safe + unsafe must fail
        assert!(!is_safe_command_windows(&vec_str(&[
            "powershell.exe",
            "-Command",
            "Get-ChildItem; Remove-Item foo",
        ])));
        // Nested unsafe cmdlet inside safe command must fail
        assert!(!is_safe_command_windows(&vec_str(&[
            "powershell.exe",
            "-Command",
            "Write-Output (Set-Content foo6.txt 'abc')",
        ])));
        // Additional nested unsafe cmdlet examples must fail
        assert!(!is_safe_command_windows(&vec_str(&[
            "powershell.exe",
            "-Command",
            "Write-Host (Remove-Item foo.txt)",
        ])));
        assert!(!is_safe_command_windows(&vec_str(&[
            "powershell.exe",
            "-Command",
            "Get-Content (New-Item bar.txt)",
        ])));

        // Unsafe @ expansion.
        assert!(!is_safe_command_windows(&vec_str(&[
            "powershell.exe",
            "-Command",
            "ls @(calc.exe)"
        ])));

        // Unsupported constructs that the AST parser refuses (no fallback to manual splitting).
        assert!(!is_safe_command_windows(&vec_str(&[
            "powershell.exe",
            "-Command",
            "ls && pwd"
        ])));

        // Sub-expressions are rejected even if they contain otherwise safe commands.
        assert!(!is_safe_command_windows(&vec_str(&[
            "powershell.exe",
            "-Command",
            "Write-Output $(Get-Content foo)"
        ])));

        // Empty words from the parser (e.g. '') are rejected.
        assert!(!is_safe_command_windows(&vec_str(&[
            "powershell.exe",
            "-Command",
            "''"
        ])));
    }

    #[test]
    fn accepts_constant_expression_arguments() {
        assert!(is_safe_command_windows(&vec_str(&[
            "powershell.exe",
            "-Command",
            "Get-Content 'foo bar'"
        ])));

        assert!(is_safe_command_windows(&vec_str(&[
            "powershell.exe",
            "-Command",
            "Get-Content \"foo bar\""
        ])));
    }

    #[test]
    fn rejects_dynamic_arguments() {
        assert!(!is_safe_command_windows(&vec_str(&[
            "powershell.exe",
            "-Command",
            "Get-Content $foo"
        ])));

        assert!(!is_safe_command_windows(&vec_str(&[
            "powershell.exe",
            "-Command",
            "Write-Output \"foo $bar\""
        ])));
    }

    #[test]
    fn uses_invoked_powershell_variant_for_parsing() {
        if !cfg!(windows) {
            return;
        }

        let chain = "pwd && ls";
        assert!(
            !is_safe_command_windows(&vec_str(&[
                "powershell.exe",
                "-NoProfile",
                "-Command",
                chain,
            ])),
            "`{chain}` is not recognized by powershell.exe"
        );

        if let Some(pwsh) = try_find_pwsh_executable_blocking() {
            assert!(
                is_safe_command_windows(&[
                    pwsh.as_path().to_str().unwrap().into(),
                    "-NoProfile".to_string(),
                    "-Command".to_string(),
                    chain.to_string(),
                ]),
                "`{chain}` should be considered safe to pwsh.exe"
            );
        }
    }
}
