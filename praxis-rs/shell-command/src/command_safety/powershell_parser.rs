use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use serde::Deserialize;
use serde::Serialize;
use std::io::BufRead;
use std::io::BufReader;
use std::io::ErrorKind;
use std::io::Write;
use std::process::Child;
use std::process::ChildStdin;
use std::process::ChildStdout;
use std::process::Command;
use std::process::Stdio;
use std::sync::LazyLock;
use std::sync::mpsc;
use std::sync::mpsc::Receiver;
use std::sync::mpsc::RecvTimeoutError;
use std::time::Duration;

const POWERSHELL_PARSER_SCRIPT: &str = include_str!("powershell_parser.ps1");
const POWERSHELL_PARSER_RESPONSE_TIMEOUT: Duration = Duration::from_secs(3);

/// Use one parser process per request so a stalled PowerShell parser cannot block unrelated tools.
pub(super) fn parse_with_powershell_ast(executable: &str, script: &str) -> PowershellParseOutcome {
    let Ok(mut parser) = PowershellParserProcess::spawn(executable) else {
        return PowershellParseOutcome::Failed;
    };
    parser
        .parse_with_timeout(script, POWERSHELL_PARSER_RESPONSE_TIMEOUT)
        .unwrap_or(PowershellParseOutcome::Failed)
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum PowershellParseOutcome {
    Commands(Vec<Vec<String>>),
    Unsupported,
    Failed,
}

fn encode_powershell_base64(script: &str) -> String {
    let mut utf16 = Vec::with_capacity(script.len() * 2);
    for unit in script.encode_utf16() {
        utf16.extend_from_slice(&unit.to_le_bytes());
    }
    BASE64_STANDARD.encode(utf16)
}

fn encoded_parser_script() -> &'static str {
    static ENCODED: LazyLock<String> =
        LazyLock::new(|| encode_powershell_base64(POWERSHELL_PARSER_SCRIPT));
    &ENCODED
}

struct PowershellParserProcess {
    child: Child,
    stdin: ChildStdin,
    responses: Receiver<std::io::Result<String>>,
    // Request ids are monotonic within one child process so the caller can detect protocol
    // desynchronization if stdout is contaminated or the child is unexpectedly replaced.
    next_request_id: u64,
}

impl PowershellParserProcess {
    fn spawn(executable: &str) -> std::io::Result<Self> {
        let child = Command::new(executable)
            .args([
                "-NoLogo",
                "-NoProfile",
                "-NonInteractive",
                "-EncodedCommand",
                encoded_parser_script(),
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        Self::from_child(child)
    }

    fn from_child(mut child: Child) -> std::io::Result<Self> {
        let stdin = match take_child_stdin(&mut child) {
            Ok(stdin) => stdin,
            Err(error) => {
                kill_child(&mut child);
                return Err(error);
            }
        };
        let stdout = match take_child_stdout(&mut child) {
            Ok(stdout) => stdout,
            Err(error) => {
                kill_child(&mut child);
                return Err(error);
            }
        };
        let responses = match spawn_response_reader(stdout) {
            Ok(responses) => responses,
            Err(error) => {
                kill_child(&mut child);
                return Err(error);
            }
        };
        Ok(Self {
            child,
            stdin,
            responses,
            next_request_id: 0,
        })
    }

    fn parse_with_timeout(
        &mut self,
        script: &str,
        response_timeout: Duration,
    ) -> std::io::Result<PowershellParseOutcome> {
        let request = PowershellParserRequest {
            id: self.next_request_id,
            payload: encode_powershell_base64(script),
        };
        self.next_request_id = self.next_request_id.wrapping_add(1);
        let mut request_json = serialize_request(&request)?;
        request_json.push('\n');
        self.stdin.write_all(request_json.as_bytes())?;
        self.stdin.flush()?;

        let response_line = match self.responses.recv_timeout(response_timeout) {
            Ok(response) => response?,
            Err(RecvTimeoutError::Timeout) => {
                return Err(std::io::Error::new(
                    ErrorKind::TimedOut,
                    format!(
                        "PowerShell parser did not respond within {} ms",
                        response_timeout.as_millis()
                    ),
                ));
            }
            Err(RecvTimeoutError::Disconnected) => {
                return Err(std::io::Error::new(
                    ErrorKind::UnexpectedEof,
                    "PowerShell parser response stream closed",
                ));
            }
        };
        if response_line.is_empty() {
            return Err(std::io::Error::new(
                ErrorKind::UnexpectedEof,
                "PowerShell parser closed stdout",
            ));
        }

        let response = deserialize_response(&response_line)?;
        // Requests are serialized today; the id still catches protocol desyncs if stdout is
        // contaminated or the child process is unexpectedly replaced mid-request. That turns an
        // ambiguous parser result into a hard failure so the caller can discard the cached child.
        if response.id != request.id {
            return Err(std::io::Error::new(
                ErrorKind::InvalidData,
                format!(
                    "PowerShell parser returned response id {} for request {}",
                    response.id, request.id
                ),
            ));
        }

        Ok(response.into_outcome())
    }

    #[cfg(all(test, windows))]
    fn spawn_unresponsive_for_test(executable: &std::path::Path) -> std::io::Result<Self> {
        let child = Command::new(executable)
            .args([
                "-NoLogo",
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "Start-Sleep -Seconds 30",
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        Self::from_child(child)
    }
}

impl Drop for PowershellParserProcess {
    fn drop(&mut self) {
        kill_child(&mut self.child);
    }
}

fn take_child_stdin(child: &mut Child) -> std::io::Result<ChildStdin> {
    child.stdin.take().ok_or_else(|| {
        std::io::Error::new(
            ErrorKind::BrokenPipe,
            "PowerShell parser child did not expose stdin",
        )
    })
}

fn take_child_stdout(child: &mut Child) -> std::io::Result<BufReader<ChildStdout>> {
    child.stdout.take().map(BufReader::new).ok_or_else(|| {
        std::io::Error::new(
            ErrorKind::BrokenPipe,
            "PowerShell parser child did not expose stdout",
        )
    })
}

fn spawn_response_reader(
    mut stdout: BufReader<ChildStdout>,
) -> std::io::Result<Receiver<std::io::Result<String>>> {
    let (responses_tx, responses_rx) = mpsc::channel();
    std::thread::Builder::new()
        .name("praxis-powershell-parser".to_string())
        .spawn(move || {
            loop {
                let mut response = String::new();
                match stdout.read_line(&mut response) {
                    Ok(0) => {
                        let _ = responses_tx.send(Err(std::io::Error::new(
                            ErrorKind::UnexpectedEof,
                            "PowerShell parser closed stdout",
                        )));
                        break;
                    }
                    Ok(_) => {
                        if responses_tx.send(Ok(response)).is_err() {
                            break;
                        }
                    }
                    Err(error) => {
                        let _ = responses_tx.send(Err(error));
                        break;
                    }
                }
            }
        })?;
    Ok(responses_rx)
}

fn serialize_request(request: &PowershellParserRequest) -> std::io::Result<String> {
    serde_json::to_string(request).map_err(|error| {
        std::io::Error::new(
            ErrorKind::InvalidData,
            format!("failed to serialize PowerShell parser request: {error}"),
        )
    })
}

fn deserialize_response(response_line: &str) -> std::io::Result<PowershellParserResponse> {
    serde_json::from_str(response_line).map_err(|error| {
        std::io::Error::new(
            ErrorKind::InvalidData,
            format!("failed to parse PowerShell parser response: {error}"),
        )
    })
}

#[derive(Serialize)]
struct PowershellParserRequest {
    id: u64,
    payload: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PowershellParserResponse {
    id: u64,
    status: String,
    commands: Option<Vec<Vec<String>>>,
}

impl PowershellParserResponse {
    fn into_outcome(self) -> PowershellParseOutcome {
        match self.status.as_str() {
            "ok" => self
                .commands
                .filter(|commands| {
                    !commands.is_empty()
                        && commands
                            .iter()
                            .all(|cmd| !cmd.is_empty() && cmd.iter().all(|word| !word.is_empty()))
                })
                .map(PowershellParseOutcome::Commands)
                .unwrap_or(PowershellParseOutcome::Unsupported),
            "unsupported" => PowershellParseOutcome::Unsupported,
            _ => PowershellParseOutcome::Failed,
        }
    }
}

fn kill_child(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

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
        let mut parser = PowershellParserProcess::spawn(powershell).unwrap();

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
        let mut parser = PowershellParserProcess::spawn(powershell).unwrap();

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
        let mut parser = PowershellParserProcess::spawn(powershell).unwrap();

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
        let mut parser = PowershellParserProcess::spawn(powershell).unwrap();

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
        let mut parser = PowershellParserProcess::spawn(powershell).unwrap();

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
        let mut parser = PowershellParserProcess::spawn(powershell).unwrap();

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
        let mut parser =
            PowershellParserProcess::spawn_unresponsive_for_test(powershell.as_path()).unwrap();
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
        let mut stalled =
            PowershellParserProcess::spawn_unresponsive_for_test(powershell.as_path()).unwrap();
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
