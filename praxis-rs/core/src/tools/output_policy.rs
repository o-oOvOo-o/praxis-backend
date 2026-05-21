use std::collections::VecDeque;
use std::sync::OnceLock;

use crate::exec::ExecToolCallOutput;

/// Default model-inline command output budget when full output is persisted as
/// an AgentOS artifact. This is intentionally conservative: the model should
/// see enough context to decide the next step, not a full build/test log.
const DEFAULT_INLINE_ARTIFACT_OUTPUT_MAX_BYTES: usize = 24 * 1024;
const DEFAULT_INLINE_ARTIFACT_HEAD_LINES: usize = 80;
const DEFAULT_INLINE_ARTIFACT_TAIL_LINES: usize = 80;
const HARD_INLINE_ARTIFACT_OUTPUT_MAX_BYTES: usize = 256 * 1024;
const HARD_INLINE_ARTIFACT_LINES: usize = 512;

static COMMAND_OUTPUT_POLICY: OnceLock<CommandOutputPolicy> = OnceLock::new();

#[derive(Clone, Copy, Debug)]
pub(crate) struct CommandOutputPolicy {
    inline_artifact_output_max_bytes: usize,
    inline_artifact_head_lines: usize,
    inline_artifact_tail_lines: usize,
}

impl CommandOutputPolicy {
    fn from_env() -> Self {
        Self {
            inline_artifact_output_max_bytes: read_usize_env(
                "PRAXIS_INLINE_ARTIFACT_OUTPUT_MAX_BYTES",
                DEFAULT_INLINE_ARTIFACT_OUTPUT_MAX_BYTES,
                HARD_INLINE_ARTIFACT_OUTPUT_MAX_BYTES,
            ),
            inline_artifact_head_lines: read_usize_env(
                "PRAXIS_INLINE_ARTIFACT_HEAD_LINES",
                DEFAULT_INLINE_ARTIFACT_HEAD_LINES,
                HARD_INLINE_ARTIFACT_LINES,
            ),
            inline_artifact_tail_lines: read_usize_env(
                "PRAXIS_INLINE_ARTIFACT_TAIL_LINES",
                DEFAULT_INLINE_ARTIFACT_TAIL_LINES,
                HARD_INLINE_ARTIFACT_LINES,
            ),
        }
    }

    fn get() -> Self {
        *COMMAND_OUTPUT_POLICY.get_or_init(Self::from_env)
    }
}

fn read_usize_env(name: &str, default_value: usize, hard_max: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .map(|value| value.min(hard_max))
        .unwrap_or(default_value)
}

pub(crate) fn apply_artifact_output_policy(
    exec_output: &ExecToolCallOutput,
    content: &str,
) -> String {
    let Some(artifact_id) = exec_output.agent_os_artifact_id.as_deref() else {
        return content.to_string();
    };

    let policy = CommandOutputPolicy::get();
    if content.len() <= policy.inline_artifact_output_max_bytes {
        return content.to_string();
    }

    let (preview, total_lines) = build_head_tail_preview(
        content,
        policy.inline_artifact_head_lines,
        policy.inline_artifact_tail_lines,
    );
    format!(
        "Command output was large and has been persisted as an AgentOS artifact.\n\
Artifact: artifact://{artifact_id}\n\
Full output bytes: {bytes}\n\
Full output lines: {lines}\n\
Inline preview: first {head} lines and last {tail} lines. Use read_agent_artifact to inspect more when needed.\n\n{preview}",
        bytes = content.len(),
        lines = total_lines,
        head = policy.inline_artifact_head_lines,
        tail = policy.inline_artifact_tail_lines,
    )
}

fn build_head_tail_preview(content: &str, head_lines: usize, tail_lines: usize) -> (String, usize) {
    let mut total_lines = 0usize;
    let mut head = Vec::with_capacity(head_lines.min(HARD_INLINE_ARTIFACT_LINES));
    let mut tail = VecDeque::with_capacity(tail_lines.min(HARD_INLINE_ARTIFACT_LINES));

    for line in content.lines() {
        total_lines += 1;
        if head.len() < head_lines {
            head.push(line);
            continue;
        }
        if tail_lines > 0 {
            if tail.len() == tail_lines {
                tail.pop_front();
            }
            tail.push_back(line);
        }
    }

    if total_lines <= head_lines + tail_lines {
        return (content.to_string(), total_lines);
    }

    let mut out =
        String::with_capacity(content.len().min(DEFAULT_INLINE_ARTIFACT_OUTPUT_MAX_BYTES));
    for line in head {
        out.push_str(line);
        out.push('\n');
    }
    out.push_str(&format!(
        "\n[... omitted {omitted} lines; full output is in artifact ...]\n\n",
        omitted = total_lines.saturating_sub(head_lines + tail_lines),
    ));
    for line in tail {
        out.push_str(line);
        out.push('\n');
    }
    (out, total_lines)
}
