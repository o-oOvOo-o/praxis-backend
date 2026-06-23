pub(crate) fn conversation_lines(prompt: Option<String>, messages: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    if let Some(p) = prompt {
        out.push("user:".to_string());
        for l in p.lines() {
            out.push(l.to_string());
        }
        out.push(String::new());
    }
    if !messages.is_empty() {
        out.push("assistant:".to_string());
        for (i, m) in messages.iter().enumerate() {
            for l in m.lines() {
                out.push(l.to_string());
            }
            if i + 1 < messages.len() {
                out.push(String::new());
            }
        }
    }
    if out.is_empty() {
        out.push("<no output>".to_string());
    }
    out
}

pub(crate) fn pretty_lines_from_error(raw: &str) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    let is_no_diff = raw.contains("No output_diff in response.");
    let is_no_msgs = raw.contains("No assistant text messages in response.");
    if is_no_diff {
        lines.push("No diff available for this task.".to_string());
    } else if is_no_msgs {
        lines.push("No assistant messages found for this task.".to_string());
    } else {
        lines.push("Failed to load task details.".to_string());
    }

    if let Some(body_idx) = raw.find(" body=")
        && let Some(json_start_rel) = raw[body_idx..].find('{')
    {
        let json_start = body_idx + json_start_rel;
        let json_str = raw[json_start..].trim();
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(json_str)
            && let Some(t) = assistant_turn(&v)
        {
            append_assistant_error(&mut lines, &t);
            if let Some(status) = t.get("turn_status").and_then(|s| s.as_str()) {
                lines.push(format!("Status: {status}"));
            }
            if let Some(text) = t
                .get("latest_event")
                .and_then(|e| e.get("text"))
                .and_then(|s| s.as_str())
                && !text.trim().is_empty()
            {
                lines.push(format!("Latest event: {}", text.trim()));
            }
        }
    }

    if lines.len() == 1 {
        let tail = if raw.len() > 320 {
            format!("{}…", &raw[..320])
        } else {
            raw.to_string()
        };
        lines.push(tail);
    } else if lines.len() >= 2 {
        if lines.iter().any(|l| l.contains("in_progress")) {
            lines.push("This task may still be running. Press 'r' to refresh.".to_string());
        }
        lines.push(String::new());
    }
    lines
}

fn assistant_turn(v: &serde_json::Value) -> Option<serde_json::Map<String, serde_json::Value>> {
    v.get("current_assistant_turn")
        .and_then(|x| x.as_object())
        .cloned()
        .or_else(|| {
            v.get("current_diff_task_turn")
                .and_then(|x| x.as_object())
                .cloned()
        })
}

fn append_assistant_error(
    lines: &mut Vec<String>,
    turn: &serde_json::Map<String, serde_json::Value>,
) {
    let Some(err) = turn.get("error").and_then(|e| e.as_object()) else {
        return;
    };
    let code = err.get("code").and_then(|s| s.as_str()).unwrap_or("");
    let msg = err.get("message").and_then(|s| s.as_str()).unwrap_or("");
    if code.is_empty() && msg.is_empty() {
        return;
    }

    let summary = if code.is_empty() {
        msg.to_string()
    } else if msg.is_empty() {
        code.to_string()
    } else {
        format!("{code}: {msg}")
    };
    lines.push(format!("Assistant error: {summary}"));
}
