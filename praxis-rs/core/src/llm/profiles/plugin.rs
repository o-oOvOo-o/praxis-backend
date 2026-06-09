use crate::model_provider_info::ModelProviderInfo;
use praxis_protocol::openai_models::ModelInfo;
use praxis_protocol::openai_models::ReasoningEffort;
use praxis_tools::ToolWebSearchBackend;

use crate::llm::prompts::LlmPromptPurpose;

use super::super::ids::BehaviorProfileId;
use crate::llm::tasks::compact::CompactExecutionPolicy;
use crate::llm::tasks::title::AutoTitleProfile;

pub(crate) type ProfileMatcher = for<'a> fn(&ProfileMatchContext<'a>) -> bool;
pub(crate) type FirstPartyProviderMatcher = fn(&str, &ModelProviderInfo) -> bool;
pub(crate) type FirstPartyModelMatcher = fn(&str) -> bool;

#[derive(Clone, Copy, Debug)]
pub(crate) struct ProfileDescriptor {
    pub(crate) id: BehaviorProfileId,
    pub(crate) label: &'static str,
    pub(crate) instructions: Option<&'static str>,
    pub(crate) prompt_layers: &'static [ProfilePromptLayerDescriptor],
    pub(crate) matcher: ProfileMatcher,
    pub(crate) provider_policy: Option<ProfileProviderPolicy>,
    pub(crate) task_policy: ProfileTaskPolicyDescriptor,
    pub(crate) tool_capabilities: ProfileToolCapabilityDescriptor,
    pub(crate) priority: i32,
}

impl ProfileDescriptor {
    pub(crate) fn matches(self, ctx: &ProfileMatchContext<'_>) -> bool {
        (self.matcher)(ctx)
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct ProfilePromptLayerDescriptor {
    pub(crate) id: &'static str,
    pub(crate) purpose: LlmPromptPurpose,
    pub(crate) content: &'static str,
}

impl ProfilePromptLayerDescriptor {
    pub(crate) const fn model_instructions(id: &'static str, content: &'static str) -> Self {
        Self {
            id,
            purpose: LlmPromptPurpose::ModelInstructions,
            content,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct ProfileProviderPolicy {
    pub(crate) canonical_provider_id: Option<&'static str>,
    pub(crate) owner_label: &'static str,
    pub(crate) provider_matches: FirstPartyProviderMatcher,
    pub(crate) model_matches: FirstPartyModelMatcher,
}

impl ProfileProviderPolicy {
    pub(crate) fn matches_provider(self, provider_id: &str, provider: &ModelProviderInfo) -> bool {
        (self.provider_matches)(provider_id, provider)
    }

    pub(crate) fn matches_model(self, model: &str) -> bool {
        (self.model_matches)(model)
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct ProfileTaskPolicyDescriptor {
    pub(crate) auto_title: Option<ProfileAutoTitlePolicyDescriptor>,
    pub(crate) compact_execution: Option<CompactExecutionPolicy>,
    pub(crate) compact_model: Option<&'static str>,
    pub(crate) auto_compact_token_limit_cap: Option<i64>,
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct ProfileToolCapabilityDescriptor {
    pub(crate) web_search_backend: Option<ToolWebSearchBackend>,
}

impl ProfileToolCapabilityDescriptor {
    pub(crate) const fn responses_web_search() -> Self {
        Self {
            web_search_backend: Some(ToolWebSearchBackend::Responses),
        }
    }

    pub(crate) const fn praxis_web_search() -> Self {
        Self {
            web_search_backend: Some(ToolWebSearchBackend::Praxis),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct ProfileAutoTitlePolicyDescriptor {
    pub(crate) model: ProfileAutoTitleModel,
    pub(crate) profile: AutoTitleProfile,
    pub(crate) reasoning_effort: Option<ReasoningEffort>,
    pub(crate) suppress_model_default_reasoning: bool,
}

impl ProfileAutoTitlePolicyDescriptor {
    pub(crate) const fn current(profile: AutoTitleProfile) -> Self {
        Self {
            model: ProfileAutoTitleModel::Current,
            profile,
            reasoning_effort: None,
            suppress_model_default_reasoning: false,
        }
    }

    pub(crate) const fn fixed_without_default_reasoning(
        model_slug: &'static str,
        profile: AutoTitleProfile,
    ) -> Self {
        Self {
            model: ProfileAutoTitleModel::Fixed(model_slug),
            profile,
            reasoning_effort: None,
            suppress_model_default_reasoning: true,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum ProfileAutoTitleModel {
    Current,
    Fixed(&'static str),
}

#[derive(Clone, Copy)]
pub(crate) struct ProfileMatchContext<'a> {
    pub(crate) model_info: &'a ModelInfo,
    pub(crate) provider_id: &'a str,
    pub(crate) provider: &'a ModelProviderInfo,
}

pub(crate) fn contains_any_text(haystacks: &[&str], needles: &[&str]) -> bool {
    haystacks.iter().any(|haystack| {
        let haystack = haystack.to_ascii_lowercase();
        needles.iter().any(|needle| haystack.contains(needle))
    })
}

pub(crate) fn base_url(provider: &ModelProviderInfo) -> &str {
    provider.base_url.as_deref().unwrap_or_default()
}
