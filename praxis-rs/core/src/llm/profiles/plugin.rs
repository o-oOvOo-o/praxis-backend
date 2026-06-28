use crate::llm::ids::WireId;
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
    #[cfg(test)]
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
    canonical_provider_id: Option<&'static str>,
    owner_label: &'static str,
    provider_matches: FirstPartyProviderMatcher,
    model_matches: FirstPartyModelMatcher,
}

impl ProfileProviderPolicy {
    pub(crate) const fn first_party(
        canonical_provider_id: &'static str,
        owner_label: &'static str,
        provider_matches: FirstPartyProviderMatcher,
        model_matches: FirstPartyModelMatcher,
    ) -> Self {
        Self {
            canonical_provider_id: Some(canonical_provider_id),
            owner_label,
            provider_matches,
            model_matches,
        }
    }

    pub(crate) fn matches_provider(self, provider_id: &str, provider: &ModelProviderInfo) -> bool {
        (self.provider_matches)(provider_id, provider)
    }

    pub(crate) fn matches_model(self, model: &str) -> bool {
        (self.model_matches)(model)
    }

    pub(crate) fn canonical_provider_id(self) -> Option<&'static str> {
        self.canonical_provider_id
    }

    pub(crate) fn owner_label(self) -> &'static str {
        self.owner_label
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct ProfileTaskPolicyDescriptor {
    pub(crate) auto_title: Option<ProfileAutoTitlePolicyDescriptor>,
    pub(crate) compact_execution: Option<CompactExecutionPolicy>,
    pub(crate) compact_model: Option<&'static str>,
    pub(crate) auto_compact_token_limit_cap: Option<i64>,
}

impl ProfileTaskPolicyDescriptor {
    pub(crate) const fn local_prompt() -> Self {
        Self {
            auto_title: None,
            compact_execution: Some(CompactExecutionPolicy::LocalPrompt),
            compact_model: None,
            auto_compact_token_limit_cap: None,
        }
    }

    pub(crate) const fn local_prompt_with_current_title(profile: AutoTitleProfile) -> Self {
        Self {
            auto_title: Some(ProfileAutoTitlePolicyDescriptor::current(profile)),
            compact_execution: Some(CompactExecutionPolicy::LocalPrompt),
            compact_model: None,
            auto_compact_token_limit_cap: None,
        }
    }

    pub(crate) const fn remote_responses_with_current_title(profile: AutoTitleProfile) -> Self {
        Self {
            auto_title: Some(ProfileAutoTitlePolicyDescriptor::current(profile)),
            compact_execution: Some(CompactExecutionPolicy::RemoteResponses),
            compact_model: None,
            auto_compact_token_limit_cap: None,
        }
    }

    pub(crate) const fn local_prompt_with_fixed_title_model(
        model_slug: &'static str,
        profile: AutoTitleProfile,
        auto_compact_token_limit_cap: Option<i64>,
    ) -> Self {
        Self {
            auto_title: Some(
                ProfileAutoTitlePolicyDescriptor::fixed_without_default_reasoning(
                    model_slug, profile,
                ),
            ),
            compact_execution: Some(CompactExecutionPolicy::LocalPrompt),
            compact_model: Some(model_slug),
            auto_compact_token_limit_cap,
        }
    }
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
    pub(crate) wire_id: WireId,
}

impl<'a> ProfileMatchContext<'a> {
    pub(crate) fn new(
        model_info: &'a ModelInfo,
        provider_id: &'a str,
        provider: &'a ModelProviderInfo,
    ) -> Self {
        Self {
            model_info,
            provider_id,
            provider,
            wire_id: WireId::from(provider.wire_api),
        }
    }

    pub(crate) fn wire_id_is(&self, wire_id: WireId) -> bool {
        self.wire_id == wire_id
    }

    pub(crate) fn provider_identity(&self) -> ProfileProviderIdentity<'a> {
        provider_identity(self.provider_id, self.provider)
    }

    pub(crate) fn provider_identity_contains_any(&self, needles: &[&str]) -> bool {
        self.provider_identity().contains_any(needles)
    }

    pub(crate) fn model_and_provider_identity_contains_any(&self, needles: &[&str]) -> bool {
        contains_any_text(
            &[
                self.model_info.slug.as_str(),
                self.provider_id,
                self.provider.name.as_str(),
                provider_base_url(self.provider),
            ],
            needles,
        )
    }
}

#[derive(Clone, Copy)]
pub(crate) struct ProfileProviderIdentity<'a> {
    provider_id: &'a str,
    provider_name: &'a str,
    base_url: &'a str,
}

impl ProfileProviderIdentity<'_> {
    pub(crate) fn id_eq(self, expected: &str) -> bool {
        self.provider_id == expected
    }

    fn id_eq_ignore_ascii_case(self, expected: &str) -> bool {
        self.provider_id.eq_ignore_ascii_case(expected)
    }

    fn name_eq_ignore_ascii_case(self, expected: &str) -> bool {
        self.provider_name.eq_ignore_ascii_case(expected)
    }

    fn name_contains_any(self, needles: &[&str]) -> bool {
        contains_any_text(&[self.provider_name], needles)
    }

    fn base_url_contains_any(self, needles: &[&str]) -> bool {
        contains_any_text(&[self.base_url], needles)
    }

    fn contains_any(self, needles: &[&str]) -> bool {
        contains_any_text(
            &[self.provider_id, self.provider_name, self.base_url],
            needles,
        )
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct ProfileProviderIdentityRule {
    exact_provider_ids: &'static [&'static str],
    provider_id_aliases: &'static [&'static str],
    provider_name_aliases: &'static [&'static str],
    provider_name_needles: &'static [&'static str],
    base_url_needles: &'static [&'static str],
}

impl ProfileProviderIdentityRule {
    pub(crate) const fn new(
        exact_provider_ids: &'static [&'static str],
        provider_id_aliases: &'static [&'static str],
        provider_name_aliases: &'static [&'static str],
        provider_name_needles: &'static [&'static str],
        base_url_needles: &'static [&'static str],
    ) -> Self {
        Self {
            exact_provider_ids,
            provider_id_aliases,
            provider_name_aliases,
            provider_name_needles,
            base_url_needles,
        }
    }

    pub(crate) fn matches_provider(self, provider_id: &str, provider: &ModelProviderInfo) -> bool {
        self.matches_identity(provider_identity(provider_id, provider))
    }

    fn matches_identity(self, identity: ProfileProviderIdentity<'_>) -> bool {
        self.exact_provider_ids
            .iter()
            .any(|expected| identity.id_eq(expected))
            || self
                .provider_id_aliases
                .iter()
                .any(|expected| identity.id_eq_ignore_ascii_case(expected))
            || self
                .provider_name_aliases
                .iter()
                .any(|expected| identity.name_eq_ignore_ascii_case(expected))
            || identity.name_contains_any(self.provider_name_needles)
            || identity.base_url_contains_any(self.base_url_needles)
    }
}

fn provider_identity<'a>(
    provider_id: &'a str,
    provider: &'a ModelProviderInfo,
) -> ProfileProviderIdentity<'a> {
    ProfileProviderIdentity {
        provider_id,
        provider_name: provider.name.as_str(),
        base_url: provider_base_url(provider),
    }
}

fn contains_any_text(haystacks: &[&str], needles: &[&str]) -> bool {
    if needles.is_empty() {
        return false;
    }

    haystacks.iter().any(|haystack| {
        let haystack = haystack.to_ascii_lowercase();
        needles.iter().any(|needle| haystack.contains(needle))
    })
}

fn provider_base_url(provider: &ModelProviderInfo) -> &str {
    provider.base_url.as_deref().unwrap_or_default()
}
