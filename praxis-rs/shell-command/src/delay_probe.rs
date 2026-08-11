/// Returns a normalized status-probe fingerprint when a command begins with a pure delay.
pub fn delay_probe_fingerprint(command: &str) -> Option<String> {
    let (delay, probe) = split_leading_statement(command)?;
    if !is_delay_statement(delay) || !is_status_probe(probe) {
        return None;
    }
    Some(normalize_fragment(probe))
}

fn split_leading_statement(command: &str) -> Option<(&str, &str)> {
    let semicolon = command.find(';').map(|index| (index, 1));
    let and_then = command.find("&&").map(|index| (index, 2));
    let (index, delimiter_len) = match (semicolon, and_then) {
        (Some(left), Some(right)) => left.min(right),
        (Some(split), None) | (None, Some(split)) => split,
        (None, None) => return None,
    };
    Some((&command[..index], &command[index + delimiter_len..]))
}

fn is_delay_statement(statement: &str) -> bool {
    let tokens = statement.split_ascii_whitespace().collect::<Vec<_>>();
    let Some(command) = tokens.first().map(|token| token.to_ascii_lowercase()) else {
        return false;
    };
    match command.as_str() {
        "start-sleep" => match tokens.as_slice() {
            [_, duration] => is_positive_duration(duration),
            [_, unit, duration] => {
                matches!(
                    unit.to_ascii_lowercase().as_str(),
                    "-seconds" | "-s" | "-milliseconds" | "-m"
                ) && is_positive_duration(duration)
            }
            _ => false,
        },
        "sleep" => matches!(tokens.as_slice(), [_, duration] if is_positive_duration(duration)),
        "timeout" => matches!(
            tokens.as_slice(),
            [_, flag, duration] | [_, flag, duration, _]
                if flag.eq_ignore_ascii_case("/t") && is_positive_duration(duration)
        ),
        _ => false,
    }
}

fn is_positive_duration(token: &str) -> bool {
    let numeric = token.trim_end_matches(|suffix: char| suffix.is_ascii_alphabetic());
    numeric
        .parse::<f64>()
        .is_ok_and(|duration| duration.is_finite() && duration > 0.0)
}

fn is_status_probe(statement: &str) -> bool {
    statement
        .split_ascii_whitespace()
        .next()
        .is_some_and(|command| {
            matches!(
                command.to_ascii_lowercase().as_str(),
                "get-process" | "get-job" | "test-path" | "ps" | "pgrep" | "jobs" | "tasklist"
            )
        })
}

fn normalize_fragment(fragment: &str) -> String {
    fragment
        .split_ascii_whitespace()
        .map(str::to_ascii_lowercase)
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn delay_duration_case_and_whitespace_do_not_change_the_fingerprint() {
        let expected = delay_probe_fingerprint(
            "Start-Sleep -Seconds 45; Get-Process cargo,rustc -ErrorAction SilentlyContinue",
        );

        assert_eq!(
            delay_probe_fingerprint(
                "  start-sleep   -Seconds 55 ;  get-process cargo,rustc -ErrorAction SilentlyContinue  "
            ),
            expected
        );
        assert_eq!(
            delay_probe_fingerprint(
                "Start-Sleep -Milliseconds 50000; GET-PROCESS cargo,rustc -ErrorAction SilentlyContinue"
            ),
            expected
        );
    }

    #[test]
    fn powershell_unix_and_cmd_delay_probes_are_recognized() {
        for command in [
            "Start-Sleep 55; Get-Job",
            "sleep 55s && pgrep cargo",
            "timeout /t 55 /nobreak && tasklist",
        ] {
            assert!(
                delay_probe_fingerprint(command).is_some(),
                "expected delay probe: {command}"
            );
        }
    }

    #[test]
    fn productive_commands_and_event_driven_waits_are_not_delay_probes() {
        for command in [
            "rg 'Start-Sleep' core/src",
            "Wait-Process -Id 42 -Timeout 300",
            "Start-Sleep -Seconds 1; Write-Output ready",
            "sleep 1; echo ready",
            "Start-Sleep -Seconds nope; Get-Process cargo",
            "Start-Sleep -Seconds 0; Get-Process cargo",
        ] {
            assert_eq!(
                delay_probe_fingerprint(command),
                None,
                "unexpected delay probe: {command}"
            );
        }
    }
}
