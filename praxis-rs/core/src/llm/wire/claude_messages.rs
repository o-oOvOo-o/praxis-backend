use crate::api_bridge::CoreAuthProvider;
use crate::client_common::Prompt;
use crate::client_common::ResponseStream;
use crate::error::Result;
use crate::model_provider_info::ModelProviderInfo;
use praxis_api::Provider;
use praxis_protocol::openai_models::ModelInfo;
use praxis_protocol::openai_models::ReasoningEffort as ReasoningEffortConfig;

pub(crate) async fn stream_unary(
    api_provider: Provider,
    api_auth: CoreAuthProvider,
    provider_info: &ModelProviderInfo,
    prompt: &Prompt,
    model_info: &ModelInfo,
    effort: Option<ReasoningEffortConfig>,
) -> Result<ResponseStream> {
    crate::non_responses_transport::stream_claude_unary(
        api_provider,
        api_auth,
        provider_info,
        prompt,
        model_info,
        effort,
    )
    .await
}
