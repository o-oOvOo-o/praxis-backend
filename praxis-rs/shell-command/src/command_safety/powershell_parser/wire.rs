use std::io;
use std::io::ErrorKind;
use std::sync::LazyLock;

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use serde::Deserialize;
use serde::Serialize;

use super::PowershellParseOutcome;

const HARNESS_SOURCE: &str = include_str!("../powershell_parser.ps1");

#[derive(Serialize)]
struct Request {
    id: u64,
    payload: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Response {
    id: u64,
    status: String,
    commands: Option<Vec<Vec<String>>>,
}

pub(super) fn encoded_harness() -> &'static str {
    static HARNESS: LazyLock<String> = LazyLock::new(|| encode(HARNESS_SOURCE));
    HARNESS.as_str()
}

pub(super) fn request_line(id: u64, source: &str) -> io::Result<String> {
    let mut line = serde_json::to_string(&Request {
        id,
        payload: encode(source),
    })
    .map_err(|error| invalid_data("encode request", error))?;
    line.push('\n');
    Ok(line)
}

pub(super) fn decode_response(line: &str, expected_id: u64) -> io::Result<PowershellParseOutcome> {
    let response: Response =
        serde_json::from_str(line).map_err(|error| invalid_data("decode response", error))?;
    if response.id != expected_id {
        return Err(io::Error::new(
            ErrorKind::InvalidData,
            format!(
                "PowerShell parser response id {} does not match request {expected_id}",
                response.id
            ),
        ));
    }
    Ok(match response.status.as_str() {
        "ok" => response
            .commands
            .filter(valid_commands)
            .map(PowershellParseOutcome::Commands)
            .unwrap_or(PowershellParseOutcome::Unsupported),
        "unsupported" => PowershellParseOutcome::Unsupported,
        _ => PowershellParseOutcome::Failed,
    })
}

fn encode(source: &str) -> String {
    let bytes: Vec<u8> = source.encode_utf16().flat_map(u16::to_le_bytes).collect();
    BASE64.encode(bytes)
}

fn valid_commands(commands: &Vec<Vec<String>>) -> bool {
    !commands.is_empty()
        && commands
            .iter()
            .all(|command| !command.is_empty() && command.iter().all(|word| !word.is_empty()))
}

fn invalid_data(context: &str, error: impl std::fmt::Display) -> io::Error {
    io::Error::new(
        ErrorKind::InvalidData,
        format!("PowerShell parser {context}: {error}"),
    )
}
