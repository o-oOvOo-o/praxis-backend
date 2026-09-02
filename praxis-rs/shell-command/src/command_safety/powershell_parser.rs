mod process;
mod wire;

use std::time::Duration;

use process::ParserSession;

const POWERSHELL_PARSER_RESPONSE_TIMEOUT: Duration = Duration::from_secs(3);

/// Parses one script in an isolated, time-bounded PowerShell AST session.
pub(super) fn parse_with_powershell_ast(executable: &str, script: &str) -> PowershellParseOutcome {
    ParserSession::spawn(executable)
        .and_then(|mut session| {
            session.parse_with_timeout(script, POWERSHELL_PARSER_RESPONSE_TIMEOUT)
        })
        .unwrap_or(PowershellParseOutcome::Failed)
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum PowershellParseOutcome {
    Commands(Vec<Vec<String>>),
    Unsupported,
    Failed,
}

#[cfg(all(test, windows))]
use std::io::ErrorKind;

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use crate::powershell::try_find_powershell_executable_blocking;
    use pretty_assertions::assert_eq;
    use std::time::Duration;
    use std::time::Instant;

    #[test]
    fn parser_process_handles_multiple_requests() {
        let Some(powershell) = try_find_powershell_executable_blocking() else {
            return;
        };
        let powershell = powershell.as_path().to_str().unwrap();
        let mut parser = ParserSession::spawn(powershell).unwrap();

        let first = parser
            .parse_with_timeout("Get-Content 'foo bar'", POWERSHELL_PARSER_RESPONSE_TIMEOUT)
            .unwrap();
        assert_eq!(
            first,
            PowershellParseOutcome::Commands(vec![vec![
                "Get-Content".to_string(),
                "foo bar".to_string(),
            ]]),
        );

        let second = parser
            .parse_with_timeout(
                "Write-Output foo | Measure-Object",
                POWERSHELL_PARSER_RESPONSE_TIMEOUT,
            )
            .unwrap();
        assert_eq!(
            second,
            PowershellParseOutcome::Commands(vec![
                vec!["Write-Output".to_string(), "foo".to_string()],
                vec!["Measure-Object".to_string()],
            ]),
        );
    }

    #[test]
    fn parser_process_rejects_stop_parsing_forms() {
        let Some(powershell) = try_find_powershell_executable_blocking() else {
            return;
        };
        let powershell = powershell.as_path().to_str().unwrap();
        let mut parser = ParserSession::spawn(powershell).unwrap();

        let parsed = parser
            .parse_with_timeout(
                "git log --% HEAD --output=codex_poc.txt",
                POWERSHELL_PARSER_RESPONSE_TIMEOUT,
            )
            .unwrap();
        assert_eq!(parsed, PowershellParseOutcome::Unsupported);
    }

    #[test]
    fn parser_process_rejects_param_blocks() {
        let Some(powershell) = try_find_powershell_executable_blocking() else {
            return;
        };
        let powershell = powershell.as_path().to_str().unwrap();
        let mut parser = ParserSession::spawn(powershell).unwrap();

        let parsed = parser
            .parse_with_timeout(
                "param([string]$path = (Get-Location)) Write-Output test",
                POWERSHELL_PARSER_RESPONSE_TIMEOUT,
            )
            .unwrap();
        assert_eq!(parsed, PowershellParseOutcome::Unsupported);
    }

    #[test]
    fn parser_process_rejects_named_blocks() {
        let Some(powershell) = try_find_powershell_executable_blocking() else {
            return;
        };
        let powershell = powershell.as_path().to_str().unwrap();
        let mut parser = ParserSession::spawn(powershell).unwrap();

        let parsed = parser
            .parse_with_timeout(
                "begin { Set-Content codex_poc.txt pwned } end { Get-Content Cargo.toml }",
                POWERSHELL_PARSER_RESPONSE_TIMEOUT,
            )
            .unwrap();
        assert_eq!(parsed, PowershellParseOutcome::Unsupported);
    }

    #[test]
    fn parser_process_rejects_using_statements() {
        let Some(powershell) = try_find_powershell_executable_blocking() else {
            return;
        };
        let powershell = powershell.as_path().to_str().unwrap();
        let mut parser = ParserSession::spawn(powershell).unwrap();

        let parsed = parser
            .parse_with_timeout(
                "using module ./codex_poc.psm1\nGet-Content Cargo.toml",
                POWERSHELL_PARSER_RESPONSE_TIMEOUT,
            )
            .unwrap();
        assert_eq!(parsed, PowershellParseOutcome::Unsupported);
    }

    #[test]
    fn parser_process_rejects_trap_blocks() {
        let Some(powershell) = try_find_powershell_executable_blocking() else {
            return;
        };
        let powershell = powershell.as_path().to_str().unwrap();
        let mut parser = ParserSession::spawn(powershell).unwrap();

        let parsed = parser
            .parse_with_timeout(
                "trap { Set-Content codex_poc.txt pwned; continue } Get-Content missing -ErrorAction Stop",
                POWERSHELL_PARSER_RESPONSE_TIMEOUT,
            )
            .unwrap();
        assert_eq!(parsed, PowershellParseOutcome::Unsupported);
    }

    #[test]
    fn parser_process_times_out_when_response_stalls() {
        let Some(powershell) = try_find_powershell_executable_blocking() else {
            return;
        };
        let mut parser = ParserSession::spawn_unresponsive_for_test(powershell.as_path()).unwrap();
        let started = Instant::now();

        let error = parser
            .parse_with_timeout("Get-Content foo.rs", Duration::from_millis(100))
            .unwrap_err();

        assert_eq!(error.kind(), ErrorKind::TimedOut);
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    #[test]
    fn fresh_parser_recovers_after_stalled_process() {
        let Some(powershell) = try_find_powershell_executable_blocking() else {
            return;
        };
        let executable = powershell.as_path().to_str().unwrap();
        let mut stalled = ParserSession::spawn_unresponsive_for_test(powershell.as_path()).unwrap();
        let error = stalled
            .parse_with_timeout("Get-Content foo.rs", Duration::from_millis(100))
            .unwrap_err();
        assert_eq!(error.kind(), ErrorKind::TimedOut);
        drop(stalled);

        let outcome = parse_with_powershell_ast(executable, "Get-Content foo.rs");
        assert_eq!(
            outcome,
            PowershellParseOutcome::Commands(vec![vec![
                "Get-Content".to_string(),
                "foo.rs".to_string(),
            ]])
        );
    }
}
