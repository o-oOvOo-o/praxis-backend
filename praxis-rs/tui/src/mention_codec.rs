use std::collections::HashMap;
use std::collections::VecDeque;

use praxis_core::mention_syntax::MentionNameMode;
use praxis_core::mention_syntax::PLUGIN_TEXT_MENTION_SIGIL;
use praxis_core::mention_syntax::TOOL_MENTION_SIGIL;
use praxis_core::mention_syntax::is_common_env_var;
use praxis_core::mention_syntax::is_tool_mention_name_char;
use praxis_core::mention_syntax::parse_linked_mention;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LinkedMention {
    pub(crate) mention: String,
    pub(crate) path: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DecodedHistoryText {
    pub(crate) text: String,
    pub(crate) mentions: Vec<LinkedMention>,
}

#[allow(dead_code)]
pub(crate) fn encode_history_mentions(text: &str, mentions: &[LinkedMention]) -> String {
    if mentions.is_empty() || text.is_empty() {
        return text.to_string();
    }

    let mut mentions_by_name: HashMap<&str, VecDeque<&str>> = HashMap::new();
    for mention in mentions {
        mentions_by_name
            .entry(mention.mention.as_str())
            .or_default()
            .push_back(mention.path.as_str());
    }

    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut index = 0usize;

    while index < bytes.len() {
        if bytes[index] == TOOL_MENTION_SIGIL as u8 {
            let name_start = index + 1;
            if let Some(first) = bytes.get(name_start)
                && is_tool_mention_name_char(*first)
            {
                let mut name_end = name_start + 1;
                while let Some(next) = bytes.get(name_end)
                    && is_tool_mention_name_char(*next)
                {
                    name_end += 1;
                }

                let name = &text[name_start..name_end];
                if let Some(path) = mentions_by_name.get_mut(name).and_then(VecDeque::pop_front) {
                    out.push('[');
                    out.push(TOOL_MENTION_SIGIL);
                    out.push_str(name);
                    out.push_str("](");
                    out.push_str(path);
                    out.push(')');
                    index = name_end;
                    continue;
                }
            }
        }

        let Some(ch) = text[index..].chars().next() else {
            break;
        };
        out.push(ch);
        index += ch.len_utf8();
    }

    out
}

pub(crate) fn decode_history_mentions(text: &str) -> DecodedHistoryText {
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut mentions = Vec::new();
    let mut index = 0usize;

    while index < bytes.len() {
        if bytes[index] == b'['
            && let Some((name, path, end_index)) = parse_history_linked_mention(text, bytes, index)
        {
            out.push(TOOL_MENTION_SIGIL);
            out.push_str(name);
            mentions.push(LinkedMention {
                mention: name.to_string(),
                path: path.to_string(),
            });
            index = end_index;
            continue;
        }

        let Some(ch) = text[index..].chars().next() else {
            break;
        };
        out.push(ch);
        index += ch.len_utf8();
    }

    DecodedHistoryText {
        text: out,
        mentions,
    }
}

fn parse_history_linked_mention<'a>(
    text: &'a str,
    text_bytes: &[u8],
    start: usize,
) -> Option<(&'a str, &'a str, usize)> {
    // TUI writes `$name`, but may read plugin `[@name](plugin://...)` links from other clients.
    if let Some(mention @ (name, path, _)) = parse_linked_mention(
        text,
        text_bytes,
        start,
        TOOL_MENTION_SIGIL,
        MentionNameMode::Tool,
    )
    .map(|m| (m.name, m.path, m.end_index))
        && !is_common_env_var(name)
        && is_tool_path(path)
    {
        return Some(mention);
    }

    if let Some(mention @ (name, path, _)) = parse_linked_mention(
        text,
        text_bytes,
        start,
        PLUGIN_TEXT_MENTION_SIGIL,
        MentionNameMode::Tool,
    )
    .map(|m| (m.name, m.path, m.end_index))
        && !is_common_env_var(name)
        && path.starts_with("plugin://")
    {
        return Some(mention);
    }

    None
}

fn is_tool_path(path: &str) -> bool {
    path.starts_with("app://")
        || path.starts_with("mcp://")
        || path.starts_with("plugin://")
        || path.starts_with("skill://")
        || path
            .rsplit(['/', '\\'])
            .next()
            .is_some_and(|name| name.eq_ignore_ascii_case("SKILL.md"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn decode_history_mentions_restores_visible_tokens() {
        let decoded = decode_history_mentions(
            "Use [$figma](app://figma-1), [$sample](plugin://sample@test), and [$figma](/tmp/figma/SKILL.md).",
        );
        assert_eq!(decoded.text, "Use $figma, $sample, and $figma.");
        assert_eq!(
            decoded.mentions,
            vec![
                LinkedMention {
                    mention: "figma".to_string(),
                    path: "app://figma-1".to_string(),
                },
                LinkedMention {
                    mention: "sample".to_string(),
                    path: "plugin://sample@test".to_string(),
                },
                LinkedMention {
                    mention: "figma".to_string(),
                    path: "/tmp/figma/SKILL.md".to_string(),
                },
            ]
        );
    }

    #[test]
    fn decode_history_mentions_restores_plugin_links_with_at_sigil() {
        let decoded = decode_history_mentions(
            "Use [@sample](plugin://sample@test) and [$figma](app://figma-1).",
        );
        assert_eq!(decoded.text, "Use $sample and $figma.");
        assert_eq!(
            decoded.mentions,
            vec![
                LinkedMention {
                    mention: "sample".to_string(),
                    path: "plugin://sample@test".to_string(),
                },
                LinkedMention {
                    mention: "figma".to_string(),
                    path: "app://figma-1".to_string(),
                },
            ]
        );
    }

    #[test]
    fn decode_history_mentions_ignores_at_sigil_for_non_plugin_paths() {
        let decoded = decode_history_mentions("Use [@figma](app://figma-1).");

        assert_eq!(decoded.text, "Use [@figma](app://figma-1).");
        assert_eq!(decoded.mentions, Vec::<LinkedMention>::new());
    }

    #[test]
    fn encode_history_mentions_links_bound_mentions_in_order() {
        let text = "$figma then $sample then $figma then $other";
        let encoded = encode_history_mentions(
            text,
            &[
                LinkedMention {
                    mention: "figma".to_string(),
                    path: "app://figma-app".to_string(),
                },
                LinkedMention {
                    mention: "sample".to_string(),
                    path: "plugin://sample@test".to_string(),
                },
                LinkedMention {
                    mention: "figma".to_string(),
                    path: "/tmp/figma/SKILL.md".to_string(),
                },
            ],
        );
        assert_eq!(
            encoded,
            "[$figma](app://figma-app) then [$sample](plugin://sample@test) then [$figma](/tmp/figma/SKILL.md) then $other"
        );
    }
}
