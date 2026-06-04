use crate::exec::ExecToolCallOutput;
use crate::exec::StreamOutput;

const REDUCED_HEAD_LINES: usize = 160;
const REDUCED_TAIL_LINES: usize = 80;
const REPEATED_LOG_MIN_COUNT: usize = 3;
const REPEATED_LOG_MIN_SAVED_LINES: usize = 12;
const REPEATED_LOG_MIN_GENERIC_COUNT: usize = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CommandOutputProfile {
    Generic,
    CargoBuild,
    CargoTest,
    GitDiff,
    Search,
    LogStream,
}

pub(crate) fn apply_command_output_reduction(raw_command: &str, output: &mut ExecToolCallOutput) {
    let profile = classify_command_output(raw_command);
    let Some(reduced) = reduce_output(profile, output) else {
        return;
    };
    output.model_output = Some(StreamOutput {
        text: reduced,
        truncated_after_lines: None,
    });
}

fn classify_command_output(raw_command: &str) -> CommandOutputProfile {
    let normalized = raw_command.to_ascii_lowercase();
    if looks_like_engine_log_command(&normalized) {
        return CommandOutputProfile::LogStream;
    }
    if contains_command_word(&normalized, "git") && contains_command_word(&normalized, "diff") {
        return CommandOutputProfile::GitDiff;
    }
    if contains_command_word(&normalized, "rg") || contains_command_word(&normalized, "grep") {
        return CommandOutputProfile::Search;
    }
    if !contains_command_word(&normalized, "cargo") {
        return CommandOutputProfile::Generic;
    }
    if contains_command_word(&normalized, "test") || contains_command_word(&normalized, "nextest") {
        return CommandOutputProfile::CargoTest;
    }
    if ["build", "check", "clippy", "run"]
        .iter()
        .any(|word| contains_command_word(&normalized, word))
    {
        return CommandOutputProfile::CargoBuild;
    }
    CommandOutputProfile::Generic
}

fn contains_command_word(command: &str, word: &str) -> bool {
    command
        .split(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_' && ch != '-')
        .any(|part| part == word)
}

fn looks_like_engine_log_command(command: &str) -> bool {
    [
        "unity",
        "unityeditor",
        "unreal",
        "unrealeditor",
        "ue4editor",
        "ue5editor",
        "runuat",
        "automationtool",
        "unrealbuildtool",
        "ubt",
    ]
    .iter()
    .any(|needle| command.contains(needle))
}

fn reduce_output(profile: CommandOutputProfile, output: &ExecToolCallOutput) -> Option<String> {
    match profile {
        CommandOutputProfile::CargoBuild | CommandOutputProfile::CargoTest => {
            reduce_cargo_output(profile, output)
        }
        CommandOutputProfile::GitDiff | CommandOutputProfile::Search => {
            reduce_head_tail_profile(profile, output)
        }
        CommandOutputProfile::LogStream => reduce_repeated_log_profile(profile, output),
        CommandOutputProfile::Generic => reduce_repeated_log_profile(profile, output),
    }
}

fn reduce_cargo_output(
    profile: CommandOutputProfile,
    output: &ExecToolCallOutput,
) -> Option<String> {
    let content = output.aggregated_output.text.as_str();
    if content.trim().is_empty() {
        return None;
    }

    let mut kept = Vec::new();
    let mut omitted_noise = 0usize;
    for line in content.lines() {
        if is_cargo_noise(profile, line) {
            omitted_noise += 1;
            continue;
        }
        kept.push(line.to_string());
    }

    if kept.is_empty() {
        kept.push(if output.exit_code == 0 {
            "cargo completed successfully with no significant diagnostics".to_string()
        } else {
            "cargo failed without retained diagnostics; inspect the raw artifact".to_string()
        });
    }

    let folded = fold_repeated_log_lines(&kept, profile);
    let (preview, omitted_preview) =
        head_tail_lines(&folded.lines, REDUCED_HEAD_LINES, REDUCED_TAIL_LINES);
    let artifact = output
        .agent_os_artifact_id
        .as_deref()
        .map(|artifact_id| {
            format!(
                "Full raw output: artifact://command-log/{artifact_id} (read_agent_artifact artifact_id=\"{artifact_id}\")\n"
            )
        })
        .unwrap_or_default();
    let profile_name = match profile {
        CommandOutputProfile::CargoBuild => "cargo-build",
        CommandOutputProfile::CargoTest => "cargo-test",
        CommandOutputProfile::GitDiff => "git-diff",
        CommandOutputProfile::Search => "search",
        CommandOutputProfile::LogStream => "log-stream",
        CommandOutputProfile::Generic => "generic",
    };
    Some(format!(
        "Praxis output profile: {profile_name}\n\
{artifact}\
Filtered cargo noise lines: {omitted_noise}\n\
Folded repeated log lines: {folded_lines} in {folded_groups} groups\n\
Filtered preview omitted lines: {omitted_preview}\n\n\
{preview}",
        folded_lines = folded.folded_lines,
        folded_groups = folded.folded_groups,
    ))
}

fn reduce_head_tail_profile(
    profile: CommandOutputProfile,
    output: &ExecToolCallOutput,
) -> Option<String> {
    let lines = output
        .aggregated_output
        .text
        .lines()
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    if lines.len() <= REDUCED_HEAD_LINES + REDUCED_TAIL_LINES {
        return None;
    }
    let (preview, omitted_preview) =
        head_tail_lines(&lines, REDUCED_HEAD_LINES, REDUCED_TAIL_LINES);
    let artifact = output
        .agent_os_artifact_id
        .as_deref()
        .map(|artifact_id| {
            format!(
                "Full raw output: artifact://command-log/{artifact_id} (read_agent_artifact artifact_id=\"{artifact_id}\")\n"
            )
        })
        .unwrap_or_default();
    let profile_name = match profile {
        CommandOutputProfile::GitDiff => "git-diff",
        CommandOutputProfile::Search => "search",
        CommandOutputProfile::CargoBuild => "cargo-build",
        CommandOutputProfile::CargoTest => "cargo-test",
        CommandOutputProfile::LogStream => "log-stream",
        CommandOutputProfile::Generic => "generic",
    };
    Some(format!(
        "Praxis output profile: {profile_name}\n\
{artifact}\
Filtered preview omitted lines: {omitted_preview}\n\n\
{preview}"
    ))
}

fn reduce_repeated_log_profile(
    profile: CommandOutputProfile,
    output: &ExecToolCallOutput,
) -> Option<String> {
    let lines = output
        .aggregated_output
        .text
        .lines()
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    if lines.is_empty() {
        return None;
    }

    let folded = fold_repeated_log_lines(&lines, profile);
    let saved_lines = folded.folded_lines.saturating_sub(folded.folded_groups);
    let large_log_stream = profile == CommandOutputProfile::LogStream
        && lines.len() > REDUCED_HEAD_LINES + REDUCED_TAIL_LINES;
    if saved_lines < REPEATED_LOG_MIN_SAVED_LINES && !large_log_stream {
        return None;
    }

    let (preview, omitted_preview) =
        head_tail_lines(&folded.lines, REDUCED_HEAD_LINES, REDUCED_TAIL_LINES);
    let artifact = output
        .agent_os_artifact_id
        .as_deref()
        .map(|artifact_id| {
            format!(
                "Full raw output: artifact://command-log/{artifact_id} (read_agent_artifact artifact_id=\"{artifact_id}\")\n"
            )
        })
        .unwrap_or_default();
    let profile_name = match profile {
        CommandOutputProfile::LogStream => "log-stream",
        CommandOutputProfile::Generic => "repeated-log",
        CommandOutputProfile::CargoBuild => "cargo-build",
        CommandOutputProfile::CargoTest => "cargo-test",
        CommandOutputProfile::GitDiff => "git-diff",
        CommandOutputProfile::Search => "search",
    };
    Some(format!(
        "Praxis output profile: {profile_name}\n\
{artifact}\
Folded repeated log lines: {folded_lines} in {folded_groups} groups\n\
Filtered preview omitted lines: {omitted_preview}\n\n\
{preview}",
        folded_lines = folded.folded_lines,
        folded_groups = folded.folded_groups,
    ))
}

fn is_cargo_noise(profile: CommandOutputProfile, line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return true;
    }
    if [
        "Blocking waiting for file lock",
        "Compiling ",
        "Checking ",
        "Fresh ",
        "Finished ",
        "Downloaded ",
        "Downloading ",
    ]
    .iter()
    .any(|prefix| trimmed.starts_with(prefix))
    {
        return true;
    }
    if profile == CommandOutputProfile::CargoTest {
        if trimmed.starts_with("running ") {
            return true;
        }
        if trimmed.starts_with("test ")
            && (trimmed.ends_with(" ... ok") || trimmed.ends_with(" ... ignored"))
        {
            return true;
        }
    }
    false
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LogSeverity {
    Error,
    Warning,
    Info,
    Debug,
    Other,
}

impl LogSeverity {
    fn from_line(line: &str) -> Self {
        let lower = line.to_ascii_lowercase();
        if [
            "error",
            "exception",
            "fatal",
            "panic",
            "crash",
            "assertion failed",
            "ensure condition failed",
        ]
        .iter()
        .any(|needle| lower.contains(needle))
        {
            return Self::Error;
        }
        if ["warning", "warn:"]
            .iter()
            .any(|needle| lower.contains(needle))
        {
            return Self::Warning;
        }
        if ["debug", "trace"]
            .iter()
            .any(|needle| lower.contains(needle))
        {
            return Self::Debug;
        }
        if ["display:", "info", "log:"]
            .iter()
            .any(|needle| lower.contains(needle))
        {
            return Self::Info;
        }
        Self::Other
    }

    fn label(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warning => "warning",
            Self::Info => "info",
            Self::Debug => "debug",
            Self::Other => "repeat",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct LogLineKey {
    severity: LogSeverity,
    message: String,
}

#[derive(Clone, Debug)]
struct LogLineSignature {
    key: LogLineKey,
    timestamp_seconds: Option<f64>,
}

#[derive(Debug)]
struct RepeatedLogFold {
    lines: Vec<String>,
    folded_lines: usize,
    folded_groups: usize,
}

#[derive(Debug)]
struct LogRun {
    key: LogLineKey,
    originals: Vec<String>,
    first_line: usize,
    last_line: usize,
    first_timestamp: Option<f64>,
    last_timestamp: Option<f64>,
}

impl LogRun {
    fn new(line: String, line_number: usize, signature: LogLineSignature) -> Self {
        Self {
            key: signature.key,
            originals: vec![line],
            first_line: line_number,
            last_line: line_number,
            first_timestamp: signature.timestamp_seconds,
            last_timestamp: signature.timestamp_seconds,
        }
    }

    fn can_absorb(&self, signature: &LogLineSignature) -> bool {
        self.key == signature.key
    }

    fn push(&mut self, line: String, line_number: usize, signature: LogLineSignature) {
        self.originals.push(line);
        self.last_line = line_number;
        if signature.timestamp_seconds.is_some() {
            self.last_timestamp = signature.timestamp_seconds;
        }
    }

    fn should_fold(&self, profile: CommandOutputProfile) -> bool {
        let min_count = match self.key.severity {
            LogSeverity::Error | LogSeverity::Warning => REPEATED_LOG_MIN_COUNT,
            LogSeverity::Info | LogSeverity::Debug
                if profile == CommandOutputProfile::LogStream =>
            {
                REPEATED_LOG_MIN_COUNT
            }
            _ => REPEATED_LOG_MIN_GENERIC_COUNT,
        };
        self.originals.len() >= min_count
    }

    fn flush(self, profile: CommandOutputProfile, folded: &mut RepeatedLogFold) {
        if self.should_fold(profile) {
            folded.folded_lines = folded.folded_lines.saturating_add(self.originals.len());
            folded.folded_groups = folded.folded_groups.saturating_add(1);
            folded.lines.push(self.summary_line());
            return;
        }
        folded.lines.extend(self.originals);
    }

    fn summary_line(&self) -> String {
        let mut fields = vec![
            format!("{} x{}", self.key.severity.label(), self.originals.len()),
            format!("lines {}-{}", self.first_line, self.last_line),
        ];
        if let (Some(first), Some(last)) = (self.first_timestamp, self.last_timestamp) {
            let duration = last - first;
            if duration >= 0.001 {
                fields.insert(1, format!("over {}", format_duration(duration)));
            }
        }
        format!("[{}] {}", fields.join(", "), self.key.message)
    }
}

fn fold_repeated_log_lines(lines: &[String], profile: CommandOutputProfile) -> RepeatedLogFold {
    let mut folded = RepeatedLogFold {
        lines: Vec::with_capacity(lines.len()),
        folded_lines: 0,
        folded_groups: 0,
    };
    let mut current: Option<LogRun> = None;

    for (index, line) in lines.iter().enumerate() {
        let line_number = index + 1;
        let Some(signature) = log_line_signature(line) else {
            if let Some(run) = current.take() {
                run.flush(profile, &mut folded);
            }
            folded.lines.push(line.clone());
            continue;
        };

        if let Some(run) = current.as_mut()
            && run.can_absorb(&signature)
        {
            run.push(line.clone(), line_number, signature);
            continue;
        }

        if let Some(run) = current.take() {
            run.flush(profile, &mut folded);
        }
        current = Some(LogRun::new(line.clone(), line_number, signature));
    }

    if let Some(run) = current {
        run.flush(profile, &mut folded);
    }
    folded
}

fn log_line_signature(line: &str) -> Option<LogLineSignature> {
    if looks_like_stack_frame(line) {
        return None;
    }
    let cleaned = strip_ansi(line);
    let timestamp_seconds = timestamp_seconds_from_line(cleaned.as_str());
    let message = normalize_log_message(cleaned.as_str());
    if message.len() < 8 {
        return None;
    }
    Some(LogLineSignature {
        key: LogLineKey {
            severity: LogSeverity::from_line(message.as_str()),
            message,
        },
        timestamp_seconds,
    })
}

fn looks_like_stack_frame(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.starts_with("at ")
        || trimmed.starts_with("at\t")
        || trimmed.starts_with("in ")
        || trimmed.starts_with("Traceback")
        || trimmed.starts_with("Stack trace")
        || trimmed.starts_with("Callstack")
        || trimmed.starts_with("--- End of stack trace")
}

fn strip_ansi(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut chars = line.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\u{1b}' && chars.peek() == Some(&'[') {
            chars.next();
            for next in chars.by_ref() {
                if next.is_ascii_alphabetic() {
                    break;
                }
            }
            continue;
        }
        out.push(ch);
    }
    out
}

fn normalize_log_message(line: &str) -> String {
    let mut text = line.trim();
    loop {
        let stripped = strip_leading_bracket_timestamp(text)
            .or_else(|| strip_leading_iso_timestamp(text))
            .or_else(|| strip_leading_clock_timestamp(text))
            .or_else(|| strip_leading_frame_number(text));
        let Some(next) = stripped else {
            break;
        };
        text = next.trim_start();
    }
    collapse_whitespace(text)
}

fn strip_leading_bracket_timestamp(text: &str) -> Option<&str> {
    let tail = text.strip_prefix('[')?;
    let end = tail.find(']')?;
    let inside = &tail[..end];
    if parse_timestamp_fragment_seconds(inside).is_none() {
        return None;
    }
    Some(&tail[end + 1..])
}

fn strip_leading_frame_number(text: &str) -> Option<&str> {
    let tail = text.strip_prefix('[')?;
    let end = tail.find(']')?;
    let inside = tail[..end].trim();
    if !inside.is_empty() && inside.chars().all(|ch| ch.is_ascii_digit()) {
        return Some(&tail[end + 1..]);
    }
    None
}

fn strip_leading_iso_timestamp(text: &str) -> Option<&str> {
    if text.len() < 19 {
        return None;
    }
    let bytes = text.as_bytes();
    let date_shape = bytes.get(4) == Some(&b'-')
        && bytes.get(7) == Some(&b'-')
        && matches!(bytes.get(10), Some(b' ') | Some(b'T'));
    if !date_shape {
        return None;
    }
    parse_time_like_seconds(text.get(11..19)?)?;
    let mut end = 19;
    while text
        .as_bytes()
        .get(end)
        .is_some_and(|byte| byte.is_ascii_digit() || matches!(byte, b'.' | b':' | b'Z' | b'z'))
    {
        end += 1;
    }
    text.get(end..)
}

fn strip_leading_clock_timestamp(text: &str) -> Option<&str> {
    if text.len() < 8 {
        return None;
    }
    parse_time_like_seconds(text.get(..8)?)?;
    let mut end = 8;
    while text
        .as_bytes()
        .get(end)
        .is_some_and(|byte| byte.is_ascii_digit() || matches!(byte, b'.' | b':'))
    {
        end += 1;
    }
    text.get(end..)
}

fn timestamp_seconds_from_line(line: &str) -> Option<f64> {
    let text = line.trim_start();
    if let Some(tail) = text.strip_prefix('[')
        && let Some(end) = tail.find(']')
        && let Some(seconds) = parse_timestamp_fragment_seconds(&tail[..end])
    {
        return Some(seconds);
    }
    if text.len() >= 19
        && text.as_bytes().get(4) == Some(&b'-')
        && text.as_bytes().get(7) == Some(&b'-')
        && matches!(text.as_bytes().get(10), Some(b' ') | Some(b'T'))
    {
        return text.get(11..19).and_then(parse_time_like_seconds);
    }
    if text.len() >= 8 {
        return text.get(..8).and_then(parse_time_like_seconds);
    }
    None
}

fn parse_timestamp_fragment_seconds(fragment: &str) -> Option<f64> {
    let candidate = fragment
        .rsplit_once('-')
        .map_or(fragment, |(_, time)| time)
        .trim();
    parse_time_like_seconds(candidate)
}

fn parse_time_like_seconds(value: &str) -> Option<f64> {
    let parts = value
        .split(|ch| ch == ':' || ch == '.')
        .take(4)
        .collect::<Vec<_>>();
    if parts.len() < 3 {
        return None;
    }
    let hour = parts[0].parse::<u32>().ok()?;
    let minute = parts[1].parse::<u32>().ok()?;
    let second = parts[2].parse::<u32>().ok()?;
    if hour > 23 || minute > 59 || second > 59 {
        return None;
    }
    let fraction = parts.get(3).and_then(|part| {
        let digits = part
            .chars()
            .take_while(char::is_ascii_digit)
            .collect::<String>();
        if digits.is_empty() {
            return None;
        }
        let value = digits.parse::<f64>().ok()?;
        Some(value / 10f64.powi(i32::try_from(digits.len()).ok()?))
    });
    Some((hour * 3600 + minute * 60 + second) as f64 + fraction.unwrap_or(0.0))
}

fn collapse_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn format_duration(seconds: f64) -> String {
    if seconds >= 10.0 {
        format!("{seconds:.1}s")
    } else {
        format!("{seconds:.3}s")
    }
}

fn head_tail_lines(lines: &[String], head: usize, tail: usize) -> (String, usize) {
    if lines.len() <= head.saturating_add(tail) {
        return (lines.join("\n"), 0);
    }
    let omitted = lines.len().saturating_sub(head + tail);
    let mut out = Vec::with_capacity(head + tail + 1);
    out.extend(lines.iter().take(head).cloned());
    out.push(format!(
        "[... omitted {omitted} filtered lines; full output is in artifact ...]"
    ));
    out.extend(lines.iter().skip(lines.len().saturating_sub(tail)).cloned());
    (out.join("\n"), omitted)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn output(text: &str, exit_code: i32) -> ExecToolCallOutput {
        ExecToolCallOutput {
            exit_code,
            aggregated_output: StreamOutput::new(text.to_string()),
            duration: Duration::from_millis(1),
            agent_os_artifact_id: Some("artifact-test".to_string()),
            ..Default::default()
        }
    }

    #[test]
    fn classifies_cargo_test() {
        assert_eq!(
            classify_command_output("cargo test -p praxis-core"),
            CommandOutputProfile::CargoTest
        );
    }

    #[test]
    fn cargo_test_reducer_keeps_failures_and_drops_pass_noise() {
        let mut output = output(
            "Compiling praxis-core\nrunning 2 tests\ntest ok_path ... ok\ntest bad_path ... FAILED\nfailures:\n---- bad_path stdout ----\nthread panicked\n",
            101,
        );
        apply_command_output_reduction("cargo test", &mut output);
        let reduced = output.model_output.expect("reduced").text;
        assert!(reduced.contains("test bad_path ... FAILED"));
        assert!(reduced.contains("thread panicked"));
        assert!(!reduced.contains("test ok_path ... ok"));
        assert!(reduced.contains("artifact://command-log/artifact-test"));
    }

    #[test]
    fn folds_repeated_unreal_warning_with_duration() {
        let mut output = output(
            "[2026.05.21-02.00.00:000][0]LogTemp: Warning: Missing texture Assets/Foo.png\n\
[2026.05.21-02.00.30:250][0]LogTemp: Warning: Missing texture Assets/Foo.png\n\
[2026.05.21-02.01.02:500][0]LogTemp: Warning: Missing texture Assets/Foo.png\n\
[2026.05.21-02.01.02:600][0]LogTemp: Warning: Missing texture Assets/Foo.png\n\
[2026.05.21-02.01.02:700][0]LogTemp: Warning: Missing texture Assets/Foo.png\n\
[2026.05.21-02.01.02:800][0]LogTemp: Warning: Missing texture Assets/Foo.png\n\
[2026.05.21-02.01.02:900][0]LogTemp: Warning: Missing texture Assets/Foo.png\n\
[2026.05.21-02.01.03:000][0]LogTemp: Warning: Missing texture Assets/Foo.png\n\
[2026.05.21-02.01.03:100][0]LogTemp: Warning: Missing texture Assets/Foo.png\n\
[2026.05.21-02.01.03:200][0]LogTemp: Warning: Missing texture Assets/Foo.png\n\
[2026.05.21-02.01.03:300][0]LogTemp: Warning: Missing texture Assets/Foo.png\n\
[2026.05.21-02.01.03:400][0]LogTemp: Warning: Missing texture Assets/Foo.png\n\
[2026.05.21-02.01.03:500][0]LogTemp: Warning: Missing texture Assets/Foo.png\n\
[2026.05.21-02.01.03:600][0]LogTemp: Warning: Missing texture Assets/Foo.png\n",
            0,
        );
        apply_command_output_reduction("UnrealEditor-Cmd.exe -run=Cook", &mut output);
        let reduced = output.model_output.expect("reduced").text;
        assert!(reduced.contains("[warning x14 over 63.6s, lines 1-14]"));
        assert!(reduced.contains("LogTemp: Warning: Missing texture Assets/Foo.png"));
        assert!(reduced.contains("Folded repeated log lines: 14 in 1 groups"));
    }

    #[test]
    fn folds_generic_repeated_warning_when_it_saves_enough_lines() {
        let repeated = std::iter::repeat("12:00:00 Warning: shader variant missing")
            .take(16)
            .collect::<Vec<_>>()
            .join("\n");
        let mut output = output(&repeated, 0);
        apply_command_output_reduction("python run_tool.py", &mut output);
        let reduced = output.model_output.expect("reduced").text;
        assert!(reduced.contains("Praxis output profile: repeated-log"));
        assert!(reduced.contains("[warning x16, lines 1-16]"));
    }

    #[test]
    fn does_not_fold_short_repetition() {
        let mut output = output(
            "Warning: shader variant missing\nWarning: shader variant missing\n",
            0,
        );
        apply_command_output_reduction("Unity.exe -batchmode", &mut output);
        assert!(output.model_output.is_none());
    }

    #[test]
    fn repeated_log_reducer_accepts_non_ascii_prefixes() {
        let mut output = output(
            "惺惺妳好 Warning: shader variant missing\n\
惺惺妳好 Warning: shader variant missing\n\
惺惺妳好 Warning: shader variant missing\n\
惺惺妳好 Warning: shader variant missing\n\
惺惺妳好 Warning: shader variant missing\n\
惺惺妳好 Warning: shader variant missing\n\
惺惺妳好 Warning: shader variant missing\n\
惺惺妳好 Warning: shader variant missing\n\
惺惺妳好 Warning: shader variant missing\n\
惺惺妳好 Warning: shader variant missing\n\
惺惺妳好 Warning: shader variant missing\n\
惺惺妳好 Warning: shader variant missing\n\
惺惺妳好 Warning: shader variant missing\n\
惺惺妳好 Warning: shader variant missing\n\
惺惺妳好 Warning: shader variant missing\n\
惺惺妳好 Warning: shader variant missing\n",
            0,
        );
        apply_command_output_reduction("python run_tool.py", &mut output);
        let reduced = output.model_output.expect("reduced").text;
        assert!(reduced.contains("[warning x16, lines 1-16]"));
    }
}
