use std::collections::HashMap;
use std::collections::HashSet;

use praxis_protocol::user_input::UserInput;

use crate::connectors;
use crate::injection::ToolMentionKind;
use crate::injection::app_id_from_path;
use crate::injection::extract_tool_mentions_with_sigil;
use crate::injection::plugin_config_name_from_path;
use crate::injection::tool_kind_for_path;
use crate::mention_syntax::PLUGIN_TEXT_MENTION_SIGIL;
use crate::mention_syntax::TOOL_MENTION_SIGIL;

use super::PluginCapabilitySummary;

pub(crate) struct CollectedToolMentions {
    pub(crate) plain_names: HashSet<String>,
    pub(crate) paths: HashSet<String>,
}

pub(crate) fn collect_tool_mentions_from_messages<I, S>(messages: I) -> CollectedToolMentions
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    collect_tool_mentions_from_messages_with_sigil(messages, TOOL_MENTION_SIGIL)
}

fn collect_tool_mentions_from_messages_with_sigil<I, S>(
    messages: I,
    sigil: char,
) -> CollectedToolMentions
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut plain_names = HashSet::new();
    let mut paths = HashSet::new();
    for message in messages {
        let mentions = extract_tool_mentions_with_sigil(message.as_ref(), sigil);
        plain_names.extend(mentions.plain_names().map(str::to_string));
        paths.extend(mentions.paths().map(str::to_string));
    }
    CollectedToolMentions { plain_names, paths }
}

pub(crate) fn collect_explicit_app_ids(input: &[UserInput]) -> HashSet<String> {
    let text_mentions =
        collect_tool_mentions_from_messages(input.iter().filter_map(|item| match item {
            UserInput::Text { text, .. } => Some(text.as_str()),
            _ => None,
        }));

    input
        .iter()
        .filter_map(|item| match item {
            UserInput::Mention { path, .. } => Some(path.clone()),
            _ => None,
        })
        .chain(text_mentions.paths)
        .filter(|path| tool_kind_for_path(path.as_str()) == ToolMentionKind::App)
        .filter_map(|path| app_id_from_path(path.as_str()).map(str::to_string))
        .collect()
}

/// Collect explicit structured or linked `plugin://...` mentions.
pub(crate) fn collect_explicit_plugin_mentions(
    input: &[UserInput],
    plugins: &[PluginCapabilitySummary],
) -> Vec<PluginCapabilitySummary> {
    if plugins.is_empty() {
        return Vec::new();
    }

    let text_mentions = collect_tool_mentions_from_messages_with_sigil(
        input.iter().filter_map(|item| match item {
            UserInput::Text { text, .. } => Some(text.as_str()),
            _ => None,
        }),
        PLUGIN_TEXT_MENTION_SIGIL,
    );

    let mentioned_config_names: HashSet<String> = input
        .iter()
        .filter_map(|item| match item {
            UserInput::Mention { path, .. } => Some(path.clone()),
            _ => None,
        })
        // Plugin plaintext links use `@`, not the default `$` tool sigil.
        .chain(text_mentions.paths)
        .filter(|path| tool_kind_for_path(path.as_str()) == ToolMentionKind::Plugin)
        .filter_map(|path| plugin_config_name_from_path(path.as_str()).map(str::to_string))
        .collect();

    if mentioned_config_names.is_empty() {
        return Vec::new();
    }

    plugins
        .iter()
        .filter(|plugin| mentioned_config_names.contains(plugin.config_name.as_str()))
        .cloned()
        .collect()
}

pub(crate) use crate::build_skill_name_counts;

pub(crate) fn build_connector_slug_counts(
    connectors: &[connectors::AppInfo],
) -> HashMap<String, usize> {
    let mut counts: HashMap<String, usize> = HashMap::new();
    for connector in connectors {
        let slug = connectors::connector_mention_slug(connector);
        *counts.entry(slug).or_insert(0) += 1;
    }
    counts
}

#[cfg(test)]
#[path = "mentions_tests.rs"]
mod tests;
