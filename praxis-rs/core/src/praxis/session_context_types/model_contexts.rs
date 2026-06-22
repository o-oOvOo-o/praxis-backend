use praxis_otel::SessionTelemetry;
use praxis_protocol::config_types::Personality;
use praxis_protocol::config_types::ServiceTier;
use praxis_protocol::openai_models::ModelInfo;
use praxis_protocol::openai_models::ReasoningEffort as ReasoningEffortConfig;

use crate::ModelProviderInfo;
use crate::auto_title_profile::AutoTitleProfile;

pub(crate) struct AutoTitleModelContext {
    pub(crate) provider_id: String,
    pub(crate) provider: ModelProviderInfo,
    pub(crate) model_info: ModelInfo,
    pub(crate) instructions: Option<String>,
    pub(crate) session_telemetry: SessionTelemetry,
    pub(crate) service_tier: Option<ServiceTier>,
    pub(crate) personality: Option<Personality>,
    pub(crate) profile: AutoTitleProfile,
    pub(crate) reasoning_effort: Option<ReasoningEffortConfig>,
}

pub(crate) struct AutoSummaryModelContext {
    pub(crate) provider_id: String,
    pub(crate) provider: ModelProviderInfo,
    pub(crate) model_info: ModelInfo,
    pub(crate) instructions: Option<String>,
    pub(crate) session_telemetry: SessionTelemetry,
    pub(crate) service_tier: Option<ServiceTier>,
    pub(crate) personality: Option<Personality>,
}
