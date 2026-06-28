use crate::api_bridge::CoreAuthProvider;
use crate::client_common::Prompt;
use crate::client_common::ResponseStream;
use crate::error::Result;
use praxis_api::Provider;
use praxis_protocol::openai_models::ModelInfo;

pub(crate) async fn stream_unary(
    api_provider: Provider,
    api_auth: CoreAuthProvider,
    prompt: &Prompt,
    model_info: &ModelInfo,
) -> Result<ResponseStream> {
    crate::non_responses_transport::stream_claude_unary(api_provider, api_auth, prompt, model_info)
        .await
}
