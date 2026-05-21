//! Sigils and parsing helpers for tool/plugin mentions in plaintext (shared across Praxis crates).

/// Default plaintext sigil for tools.
pub const TOOL_MENTION_SIGIL: char = '$';

/// Plugins use `@` in linked plaintext outside TUI.
pub const PLUGIN_TEXT_MENTION_SIGIL: char = '@';

/// Name-character policy for mention parsing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MentionNameMode {
    /// Runtime tool/app/plugin mentions. Keep this deliberately conservative so
    /// `$foo:bar` does not accidentally bind to `$foo` UI mentions.
    Tool,
    /// Skill injection syntax historically allowed `:` in skill names.
    Skill,
}

/// Parsed linked mention of the form `[$name](path)` or `[@name](path)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LinkedMention<'a> {
    pub name: &'a str,
    pub path: &'a str,
    pub end_index: usize,
}

pub fn parse_linked_mention<'a>(
    text: &'a str,
    text_bytes: &[u8],
    start: usize,
    sigil: char,
    mode: MentionNameMode,
) -> Option<LinkedMention<'a>> {
    let sigil_index = start + 1;
    if text_bytes.get(sigil_index) != Some(&(sigil as u8)) {
        return None;
    }

    let name_start = sigil_index + 1;
    let first_name_byte = text_bytes.get(name_start)?;
    if !is_mention_name_char(*first_name_byte, mode) {
        return None;
    }

    let mut name_end = name_start + 1;
    while let Some(next_byte) = text_bytes.get(name_end)
        && is_mention_name_char(*next_byte, mode)
    {
        name_end += 1;
    }

    if text_bytes.get(name_end) != Some(&b']') {
        return None;
    }

    let mut path_start = name_end + 1;
    while let Some(next_byte) = text_bytes.get(path_start)
        && next_byte.is_ascii_whitespace()
    {
        path_start += 1;
    }
    if text_bytes.get(path_start) != Some(&b'(') {
        return None;
    }

    let mut path_end = path_start + 1;
    while let Some(next_byte) = text_bytes.get(path_end)
        && *next_byte != b')'
    {
        path_end += 1;
    }
    if text_bytes.get(path_end) != Some(&b')') {
        return None;
    }

    let path = text[path_start + 1..path_end].trim();
    if path.is_empty() {
        return None;
    }

    Some(LinkedMention {
        name: &text[name_start..name_end],
        path,
        end_index: path_end + 1,
    })
}

pub fn is_tool_mention_name_char(byte: u8) -> bool {
    is_mention_name_char(byte, MentionNameMode::Tool)
}

pub fn is_skill_mention_name_char(byte: u8) -> bool {
    is_mention_name_char(byte, MentionNameMode::Skill)
}

fn is_mention_name_char(byte: u8, mode: MentionNameMode) -> bool {
    matches!(byte, b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'_' | b'-')
        || (mode == MentionNameMode::Skill && byte == b':')
}

pub fn is_common_env_var(name: &str) -> bool {
    let upper = name.to_ascii_uppercase();
    matches!(
        upper.as_str(),
        "PATH"
            | "HOME"
            | "USER"
            | "SHELL"
            | "PWD"
            | "TMPDIR"
            | "TEMP"
            | "TMP"
            | "LANG"
            | "TERM"
            | "XDG_CONFIG_HOME"
    )
}
