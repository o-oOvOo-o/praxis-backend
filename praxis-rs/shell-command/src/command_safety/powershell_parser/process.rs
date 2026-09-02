use std::io;
use std::io::BufRead;
use std::io::BufReader;
use std::io::ErrorKind;
use std::io::Read;
use std::io::Write;
use std::process::Child;
use std::process::ChildStdin;
use std::process::ChildStdout;
use std::process::Command;
use std::process::Stdio;
use std::sync::mpsc;
use std::sync::mpsc::Receiver;
use std::sync::mpsc::RecvTimeoutError;
use std::time::Duration;

use super::PowershellParseOutcome;
use super::wire;

const MAX_RESPONSE_BYTES: u64 = 256 * 1024;

pub(super) struct ParserSession {
    child: Child,
    input: ChildStdin,
    responses: Receiver<io::Result<String>>,
    sequence: u64,
}

impl ParserSession {
    pub(super) fn spawn(executable: &str) -> io::Result<Self> {
        let child = Command::new(executable)
            .args([
                "-NoLogo",
                "-NoProfile",
                "-NonInteractive",
                "-EncodedCommand",
                wire::encoded_harness(),
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        Self::attach(child)
    }

    fn attach(mut child: Child) -> io::Result<Self> {
        let (input, output) = match Self::take_pipes(&mut child) {
            Ok(pipes) => pipes,
            Err(error) => {
                terminate(&mut child);
                return Err(error);
            }
        };
        let responses = match response_pump(output) {
            Ok(responses) => responses,
            Err(error) => {
                terminate(&mut child);
                return Err(error);
            }
        };
        Ok(Self {
            child,
            input,
            responses,
            sequence: 0,
        })
    }

    fn take_pipes(child: &mut Child) -> io::Result<(ChildStdin, BufReader<ChildStdout>)> {
        let input = child.stdin.take().ok_or_else(|| missing_pipe("stdin"))?;
        let output = child
            .stdout
            .take()
            .map(BufReader::new)
            .ok_or_else(|| missing_pipe("stdout"))?;
        Ok((input, output))
    }

    pub(super) fn parse_with_timeout(
        &mut self,
        source: &str,
        timeout: Duration,
    ) -> io::Result<PowershellParseOutcome> {
        let request_id = self.sequence;
        self.sequence = self.sequence.wrapping_add(1);
        self.input
            .write_all(wire::request_line(request_id, source)?.as_bytes())?;
        self.input.flush()?;
        let line = match self.responses.recv_timeout(timeout) {
            Ok(line) => line?,
            Err(RecvTimeoutError::Timeout) => {
                return Err(io::Error::new(
                    ErrorKind::TimedOut,
                    format!("PowerShell parser exceeded {} ms", timeout.as_millis()),
                ));
            }
            Err(RecvTimeoutError::Disconnected) => {
                return Err(io::Error::new(
                    ErrorKind::UnexpectedEof,
                    "PowerShell parser response stream disconnected",
                ));
            }
        };
        wire::decode_response(&line, request_id)
    }

    #[cfg(all(test, windows))]
    pub(super) fn spawn_unresponsive_for_test(executable: &std::path::Path) -> io::Result<Self> {
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
        Self::attach(child)
    }
}

impl Drop for ParserSession {
    fn drop(&mut self) {
        terminate(&mut self.child);
    }
}

fn response_pump(mut output: BufReader<ChildStdout>) -> io::Result<Receiver<io::Result<String>>> {
    let (sender, receiver) = mpsc::sync_channel(1);
    std::thread::Builder::new()
        .name("praxis-powershell-ast".to_owned())
        .spawn(move || {
            loop {
                let response = read_response(&mut output);
                let terminal = response.is_err();
                if sender.send(response).is_err() || terminal {
                    break;
                }
            }
        })?;
    Ok(receiver)
}

fn read_response(output: &mut BufReader<ChildStdout>) -> io::Result<String> {
    let mut line = String::new();
    let bytes = output
        .by_ref()
        .take(MAX_RESPONSE_BYTES + 1)
        .read_line(&mut line)?;
    match bytes as u64 {
        0 => Err(io::Error::new(
            ErrorKind::UnexpectedEof,
            "PowerShell parser closed stdout",
        )),
        length if length > MAX_RESPONSE_BYTES => Err(io::Error::new(
            ErrorKind::InvalidData,
            "PowerShell parser response exceeded its byte budget",
        )),
        _ => Ok(line),
    }
}

fn missing_pipe(name: &str) -> io::Error {
    io::Error::new(
        ErrorKind::BrokenPipe,
        format!("PowerShell parser child has no {name}"),
    )
}

fn terminate(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}
