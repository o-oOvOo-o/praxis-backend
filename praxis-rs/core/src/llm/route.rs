use super::ids::BehaviorProfileId;
use super::ids::WireId;
use super::profiles::plugin::ProfileMatchContext;
use super::registry::LlmProfileRegistry;
use crate::model_provider_info::ModelProviderInfo;
use praxis_protocol::openai_models::ModelInfo;

pub(crate) struct LlmRouteInput<'a> {
    pub(crate) model_info: &'a ModelInfo,
    pub(crate) provider_id: &'a str,
    pub(crate) provider: &'a ModelProviderInfo,
}

impl<'a> LlmRouteInput<'a> {
    pub(crate) fn profile_context(&self) -> ProfileMatchContext<'a> {
        ProfileMatchContext::new(self.model_info, self.provider_id, self.provider)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LlmRoute {
    pub(crate) wire: WireId,
    pub(crate) behavior: Option<BehaviorProfileId>,
}

impl LlmRoute {
    pub(crate) fn resolve(input: &LlmRouteInput<'_>) -> Self {
        let ctx = input.profile_context();
        Self {
            wire: wire_id_for_provider(input.provider),
            behavior: LlmProfileRegistry::builtin_static()
                .resolve(&ctx)
                .map(|profile| profile.id),
        }
    }
}

pub(crate) fn wire_id_for_provider(provider: &ModelProviderInfo) -> WireId {
    WireId::from(provider.wire_api)
}
