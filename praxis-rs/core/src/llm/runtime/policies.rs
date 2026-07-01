use std::collections::HashSet;
use std::fs;
use std::path::Path;

use praxis_protocol::openai_models::ReasoningEffort;
use praxis_tools::ToolCapabilityConfig;
use praxis_tools::ToolWebSearchBackend;
use serde::Deserialize;

use super::normalization::normalize_non_empty_string;
use super::normalization::normalize_non_empty_tool_name;
use super::normalization::normalize_selector;
use super::normalization::normalize_tool_name;
use crate::llm::ids::BehaviorProfileId;
use crate::llm::profiles::plugin::ProfileAutoTitleModel;
use crate::llm::profiles::plugin::ProfileTaskPolicyDescriptor;
use crate::llm::tasks::compact::CompactExecutionPolicy;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct LlmToolVisibilityPolicy {
    visible_tools: Option<HashSet<String>>,
    hidden_tools: HashSet<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawLlmToolVisibilityPolicy {
    #[serde(
        default,
        alias = "visible_tools",
        alias = "allowed_tools",
        alias = "allow"
    )]
    visible_tools: Option<Vec<String>>,
    #[serde(
        default,
        alias = "hidden_tools",
        alias = "denied_tools",
        alias = "deny"
    )]
    hidden_tools: Vec<String>,
    #[serde(
        default,
        alias = "web_search",
        alias = "webSearch",
        alias = "web_search_backend",
        alias = "webSearchBackend"
    )]
    web_search_backend: Option<RawLlmWebSearchBackend>,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum RawLlmWebSearchBackend {
    Responses,
    Praxis,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct LlmAutoTitleTaskPolicy {
    pub(crate) model_slug: Option<String>,
    pub(crate) reasoning_effort: Option<ReasoningEffort>,
    pub(crate) suppress_model_default_reasoning: Option<bool>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct LlmTaskPolicy {
    pub(super) auto_title: Option<LlmAutoTitleTaskPolicy>,
    pub(super) compact_execution: Option<CompactExecutionPolicy>,
    pub(super) compact_model: Option<String>,
    pub(super) auto_compact_token_limit_cap: Option<i64>,
}

impl LlmAutoTitleTaskPolicy {
    fn merge(&mut self, other: Self) {
        if other.model_slug.is_some() {
            self.model_slug = other.model_slug;
        }
        if other.reasoning_effort.is_some() {
            self.reasoning_effort = other.reasoning_effort;
        }
        if other.suppress_model_default_reasoning.is_some() {
            self.suppress_model_default_reasoning = other.suppress_model_default_reasoning;
        }
    }
}

impl LlmTaskPolicy {
    pub(super) fn from_profile_descriptor(descriptor: ProfileTaskPolicyDescriptor) -> Self {
        Self {
            auto_title: descriptor
                .auto_title
                .map(|auto_title| LlmAutoTitleTaskPolicy {
                    model_slug: match auto_title.model {
                        ProfileAutoTitleModel::Current => None,
                        ProfileAutoTitleModel::Fixed(model_slug) => Some(model_slug.to_string()),
                    },
                    reasoning_effort: auto_title.reasoning_effort,
                    suppress_model_default_reasoning: Some(
                        auto_title.suppress_model_default_reasoning,
                    ),
                }),
            compact_execution: descriptor.compact_execution,
            compact_model: descriptor.compact_model.map(str::to_string),
            auto_compact_token_limit_cap: descriptor.auto_compact_token_limit_cap,
        }
    }

    pub(super) fn is_empty(&self) -> bool {
        self.auto_title.is_none()
            && self.compact_execution.is_none()
            && self.compact_model.is_none()
            && self.auto_compact_token_limit_cap.is_none()
    }

    pub(super) fn merge(&mut self, other: Self) {
        if let Some(other_auto_title) = other.auto_title {
            self.auto_title
                .get_or_insert_with(LlmAutoTitleTaskPolicy::default)
                .merge(other_auto_title);
        }
        if other.compact_execution.is_some() {
            self.compact_execution = other.compact_execution;
        }
        if other.compact_model.is_some() {
            self.compact_model = other.compact_model;
        }
        if other.auto_compact_token_limit_cap.is_some() {
            self.auto_compact_token_limit_cap = other.auto_compact_token_limit_cap;
        }
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawLlmTaskPolicy {
    #[serde(default, alias = "auto_title")]
    auto_title: Option<RawAutoTitleTaskPolicy>,
    #[serde(default)]
    compact: Option<RawCompactTaskPolicy>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawAutoTitleTaskPolicy {
    #[serde(default, alias = "model_slug")]
    model: Option<String>,
    #[serde(default, alias = "reasoning_effort")]
    reasoning_effort: Option<ReasoningEffort>,
    #[serde(
        default,
        alias = "suppress_model_default_reasoning",
        alias = "suppressDefaultReasoning"
    )]
    suppress_model_default_reasoning: Option<bool>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawCompactTaskPolicy {
    #[serde(default)]
    execution: Option<String>,
    #[serde(
        default,
        alias = "model_slug",
        alias = "modelSlug",
        alias = "compact_model",
        alias = "compactModel"
    )]
    model: Option<String>,
    #[serde(
        default,
        alias = "auto_compact_token_limit",
        alias = "autoCompactTokenLimit",
        alias = "token_limit_cap",
        alias = "tokenLimitCap"
    )]
    auto_compact_token_limit_cap: Option<i64>,
}

impl LlmToolVisibilityPolicy {
    pub(crate) fn allow_none() -> Self {
        Self {
            visible_tools: Some(HashSet::new()),
            hidden_tools: HashSet::new(),
        }
    }

    #[cfg(test)]
    pub(crate) fn from_tool_names(visible_tools: Option<&[&str]>, hidden_tools: &[&str]) -> Self {
        Self {
            visible_tools: visible_tools.map(|tools| {
                tools
                    .iter()
                    .filter_map(|tool| normalize_non_empty_tool_name(tool))
                    .collect()
            }),
            hidden_tools: hidden_tools
                .iter()
                .filter_map(|tool| normalize_non_empty_tool_name(tool))
                .collect(),
        }
    }

    pub(super) fn is_empty(&self) -> bool {
        self.visible_tools
            .as_ref()
            .is_none_or(|tools| tools.is_empty())
            && self.hidden_tools.is_empty()
    }

    pub(crate) fn allows(&self, tool_name: &str) -> bool {
        let tool_name = normalize_tool_name(tool_name);
        if tool_name.is_empty() {
            return false;
        }
        if self.hidden_tools.contains(&tool_name) {
            return false;
        }
        self.visible_tools
            .as_ref()
            .is_none_or(|visible_tools| visible_tools.contains(&tool_name))
    }

    pub(super) fn merge(&mut self, other: Self) {
        if let Some(other_visible_tools) = other.visible_tools {
            self.visible_tools
                .get_or_insert_with(HashSet::new)
                .extend(other_visible_tools);
        }
        self.hidden_tools.extend(other.hidden_tools);
    }
}

pub(super) fn read_tool_visibility_policy(
    path: &Path,
    behavior_id: BehaviorProfileId,
    policy_id: &str,
) -> Option<LlmToolVisibilityPolicy> {
    let contents = match fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(err) => {
            tracing::warn!(
                path = %path.display(),
                prompt_profile = behavior_id.as_str(),
                tool_policy = policy_id,
                "failed to read plugin LLM tool policy: {err}"
            );
            return None;
        }
    };
    let raw = match toml::from_str::<RawLlmToolVisibilityPolicy>(&contents) {
        Ok(raw) => raw,
        Err(err) => {
            tracing::warn!(
                path = %path.display(),
                prompt_profile = behavior_id.as_str(),
                tool_policy = policy_id,
                "failed to parse plugin LLM tool policy: {err}"
            );
            return None;
        }
    };

    let visible_tools = raw.visible_tools.map(|tools| {
        tools
            .into_iter()
            .filter_map(|tool| normalize_non_empty_tool_name(&tool))
            .collect::<HashSet<_>>()
    });
    let hidden_tools = raw
        .hidden_tools
        .into_iter()
        .filter_map(|tool| normalize_non_empty_tool_name(&tool))
        .collect::<HashSet<_>>();
    let policy = LlmToolVisibilityPolicy {
        visible_tools,
        hidden_tools,
    };

    (!policy.is_empty()).then_some(policy)
}

pub(super) fn read_tool_capability_policy(
    path: &Path,
    behavior_id: BehaviorProfileId,
    policy_id: &str,
) -> Option<ToolCapabilityConfig> {
    let contents = match fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(err) => {
            tracing::warn!(
                path = %path.display(),
                prompt_profile = behavior_id.as_str(),
                tool_policy = policy_id,
                "failed to read plugin LLM tool capabilities: {err}"
            );
            return None;
        }
    };
    let raw = match toml::from_str::<RawLlmToolVisibilityPolicy>(&contents) {
        Ok(raw) => raw,
        Err(err) => {
            tracing::warn!(
                path = %path.display(),
                prompt_profile = behavior_id.as_str(),
                tool_policy = policy_id,
                "failed to parse plugin LLM tool capabilities: {err}"
            );
            return None;
        }
    };
    let capabilities = ToolCapabilityConfig {
        web_search_backend: raw.web_search_backend.map(|backend| match backend {
            RawLlmWebSearchBackend::Responses => ToolWebSearchBackend::Responses,
            RawLlmWebSearchBackend::Praxis => ToolWebSearchBackend::Praxis,
        }),
    };
    capabilities
        .web_search_backend
        .is_some()
        .then_some(capabilities)
}

pub(super) fn merge_tool_capabilities(
    target: &mut ToolCapabilityConfig,
    source: ToolCapabilityConfig,
) {
    if source.web_search_backend.is_some() {
        target.web_search_backend = source.web_search_backend;
    }
}

pub(super) fn read_task_policy(
    path: &Path,
    behavior_id: BehaviorProfileId,
) -> Option<LlmTaskPolicy> {
    let contents = match fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(err) => {
            tracing::warn!(
                path = %path.display(),
                prompt_profile = behavior_id.as_str(),
                "failed to read plugin LLM task policy: {err}"
            );
            return None;
        }
    };
    let raw = match toml::from_str::<RawLlmTaskPolicy>(&contents) {
        Ok(raw) => raw,
        Err(err) => {
            tracing::warn!(
                path = %path.display(),
                prompt_profile = behavior_id.as_str(),
                "failed to parse plugin LLM task policy: {err}"
            );
            return None;
        }
    };

    let auto_title = raw.auto_title.and_then(|policy| {
        let model_slug = policy
            .model
            .and_then(|model| normalize_non_empty_string(&model));
        let normalized = LlmAutoTitleTaskPolicy {
            model_slug,
            reasoning_effort: policy.reasoning_effort,
            suppress_model_default_reasoning: policy.suppress_model_default_reasoning,
        };
        (normalized.model_slug.is_some()
            || normalized.reasoning_effort.is_some()
            || normalized.suppress_model_default_reasoning.is_some())
        .then_some(normalized)
    });
    let (compact_execution, compact_model, auto_compact_token_limit_cap) = raw
        .compact
        .map(|policy| {
            (
                policy
                    .execution
                    .and_then(|execution| parse_compact_execution_policy(&execution)),
                policy
                    .model
                    .and_then(|model| normalize_non_empty_string(&model)),
                normalize_compact_token_limit_cap(policy.auto_compact_token_limit_cap),
            )
        })
        .unwrap_or_default();
    let policy = LlmTaskPolicy {
        auto_title,
        compact_execution,
        compact_model,
        auto_compact_token_limit_cap,
    };

    (!policy.is_empty()).then_some(policy)
}

fn parse_compact_execution_policy(value: &str) -> Option<CompactExecutionPolicy> {
    match normalize_selector(value).as_str() {
        "remote" | "remote_responses" | "responses" => {
            Some(CompactExecutionPolicy::RemoteResponses)
        }
        "local" | "local_prompt" | "prompt" => Some(CompactExecutionPolicy::LocalPrompt),
        _ => {
            tracing::warn!("ignoring unknown compact execution policy `{value}`");
            None
        }
    }
}

fn normalize_compact_token_limit_cap(value: Option<i64>) -> Option<i64> {
    match value {
        Some(value) if value > 0 => Some(value),
        Some(value) => {
            tracing::warn!("ignoring non-positive compact token limit cap `{value}`");
            None
        }
        None => None,
    }
}
