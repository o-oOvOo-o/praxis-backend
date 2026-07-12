use async_trait::async_trait;
use futures::StreamExt;
use praxis_api::ReqwestTransport;
use praxis_api::ResponseEvent;
use praxis_api::ResponsesApiRequest;
use praxis_api::ResponsesClient;
use praxis_api::ResponsesOptions;
use praxis_api::requests::responses::Compression;
use praxis_login::default_client::build_reqwest_client;
use praxis_protocol::models::ContentItem;
use praxis_protocol::models::ResponseItem;
use praxis_protocol::openai_models::IMAGE_GENERATION_TOOL_NAME;
use praxis_protocol::protocol::EventMsg;
use praxis_protocol::protocol::ImageGenerationBeginEvent;
use praxis_protocol::protocol::ImageGenerationEndEvent;
use praxis_tools::create_image_generation_tool;
use praxis_tools::create_tools_json_for_responses_api;
use serde::Deserialize;

use crate::function_tool::FunctionCallError;
use crate::model_provider_info::ModelProviderInfo;
use crate::model_provider_info::OPENAI_PROVIDER_ID;
use crate::provider_decision_center::AuthRequestPurpose;
use crate::provider_decision_center::ProviderDecisionCenter;
use crate::tools::context::FunctionToolOutput;
use crate::tools::context::ToolInvocation;
use crate::tools::context::ToolPayload;
use crate::tools::handlers::parse_arguments;
use crate::tools::registry::ToolHandler;
use crate::tools::registry::ToolKind;
use crate::turn_image_output::save_image_generation_result;

const ROUTED_IMAGE_MODEL: &str = "gpt-5.5";

pub struct ImageGenerationHandler;

#[derive(Debug, Deserialize)]
struct ImageGenerationArgs {
    prompt: String,
    size: Option<String>,
    quality: Option<String>,
}

struct GeneratedImage {
    status: String,
    revised_prompt: Option<String>,
    result: String,
}

#[async_trait]
impl ToolHandler for ImageGenerationHandler {
    type Output = FunctionToolOutput;

    fn kind(&self) -> ToolKind {
        ToolKind::Function
    }

    async fn handle(&self, invocation: ToolInvocation) -> Result<Self::Output, FunctionCallError> {
        let ToolInvocation {
            session,
            turn,
            call_id,
            payload,
            ..
        } = invocation;

        let arguments = match payload {
            ToolPayload::Function { arguments } => arguments,
            _ => {
                return Err(FunctionCallError::Fatal(
                    "image_generation handler received unsupported payload".to_string(),
                ));
            }
        };
        let args: ImageGenerationArgs = parse_arguments(&arguments)?;
        let prompt = image_prompt(&args)?;

        session
            .send_event(
                turn.as_ref(),
                EventMsg::ImageGenerationBegin(ImageGenerationBeginEvent {
                    call_id: call_id.clone(),
                }),
            )
            .await;

        let image =
            match run_routed_image_generation(session.as_ref(), turn.as_ref(), &prompt).await {
                Ok(image) => image,
                Err(err) => {
                    emit_image_generation_failure(session.as_ref(), turn.as_ref(), &call_id).await;
                    return Err(FunctionCallError::RespondToModel(err));
                }
            };

        let session_id = session.conversation_id.to_string();
        let saved_path = match save_image_generation_result(
            turn.config.praxis_home.as_path(),
            &session_id,
            &call_id,
            &image.result,
        )
        .await
        {
            Ok(path) => Some(path.to_string_lossy().into_owned()),
            Err(err) => {
                tracing::warn!(
                    call_id = %call_id,
                    thread_id = %session_id,
                    "failed to save routed generated image: {err}"
                );
                None
            }
        };

        session
            .send_event(
                turn.as_ref(),
                EventMsg::ImageGenerationEnd(ImageGenerationEndEvent {
                    call_id: call_id.clone(),
                    status: image.status,
                    revised_prompt: image.revised_prompt,
                    result: image.result,
                    saved_path: saved_path.clone(),
                }),
            )
            .await;

        let response = saved_path.map_or_else(
            || "Image generated, but Praxis could not save it to a local file.".to_string(),
            |path| format!("Image generated and saved to {path}."),
        );
        Ok(FunctionToolOutput::from_text(response, Some(true)))
    }
}

fn image_prompt(args: &ImageGenerationArgs) -> Result<String, FunctionCallError> {
    let prompt = args.prompt.trim();
    if prompt.is_empty() {
        return Err(FunctionCallError::RespondToModel(
            "image_generation requires a non-empty `prompt`".to_string(),
        ));
    }

    let mut sections = vec![prompt.to_string()];
    if let Some(size) = args
        .size
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        sections.push(format!("Requested size or aspect ratio: {size}."));
    }
    if let Some(quality) = args
        .quality
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        sections.push(format!("Requested quality: {quality}."));
    }
    Ok(sections.join("\n"))
}

async fn run_routed_image_generation(
    session: &crate::praxis::Session,
    turn: &crate::praxis::TurnContext,
    prompt: &str,
) -> Result<GeneratedImage, String> {
    let provider = openai_provider(turn);
    let auth_manager = ProviderDecisionCenter::provider_auth_manager(
        Some(session.services.auth_manager.clone()),
        &provider,
    );
    let setup = ProviderDecisionCenter::new(auth_manager)
        .setup_provider(
            crate::model_provider_info::OPENAI_PROVIDER_ID,
            &provider,
            AuthRequestPurpose::ModelTurn,
        )
        .await
        .map_err(|err| format!("image_generation auth setup failed: {err}"))?;

    if setup.auth_mode.is_none() {
        return Err(
            "image_generation requires OpenAI auth or an OpenAI API key. Log in to Praxis or set OPENAI_API_KEY."
                .to_string(),
        );
    }

    let route_request_id = format!("{}:image_generation", session.conversation_id);
    let request = routed_image_request(turn, &setup.api_provider, prompt, &route_request_id)
        .map_err(|err| format!("failed to build image_generation request: {err}"))?;
    let options = ResponsesOptions {
        conversation_id: Some(route_request_id),
        session_source: Some(turn.session_source.clone()),
        compression: Compression::None,
        ..Default::default()
    };
    let client = ResponsesClient::new(
        ReqwestTransport::new(build_reqwest_client()),
        setup.api_provider,
        setup.api_auth,
    );
    let mut stream = client
        .stream_request(request, options)
        .await
        .map_err(|err| format!("image_generation request failed: {err}"))?;

    let mut text_response = String::new();
    while let Some(event) = stream.next().await {
        let event = event.map_err(|err| format!("image_generation stream failed: {err}"))?;
        match event {
            ResponseEvent::OutputItemDone(ResponseItem::ImageGenerationCall {
                status,
                revised_prompt,
                result,
                ..
            }) => {
                return Ok(GeneratedImage {
                    status,
                    revised_prompt,
                    result,
                });
            }
            ResponseEvent::OutputTextDelta(delta) => {
                text_response.push_str(&delta);
            }
            ResponseEvent::Completed { .. } => break,
            _ => {}
        }
    }

    let suffix = if text_response.trim().is_empty() {
        String::new()
    } else {
        format!(" Model response: {}", text_response.trim())
    };
    Err(format!(
        "image_generation did not return an image_generation_call result.{suffix}"
    ))
}

fn openai_provider(turn: &crate::praxis::TurnContext) -> ModelProviderInfo {
    turn.config
        .model_providers
        .get(OPENAI_PROVIDER_ID)
        .cloned()
        .unwrap_or_else(|| ModelProviderInfo::create_openai_provider(None))
}

fn routed_image_request(
    turn: &crate::praxis::TurnContext,
    provider: &praxis_api::Provider,
    prompt: &str,
    route_request_id: &str,
) -> Result<ResponsesApiRequest, serde_json::Error> {
    let input = vec![ResponseItem::Message {
        id: None,
        role: "user".to_string(),
        content: vec![ContentItem::InputText {
            text: prompt.to_string(),
        }],
        end_turn: None,
        phase: None,
    }];
    let tools = create_tools_json_for_responses_api(&[create_image_generation_tool("png")])?;
    let model = routed_model_slug(turn);

    Ok(ResponsesApiRequest {
        model,
        instructions: format!(
            "Generate exactly one image with the `{IMAGE_GENERATION_TOOL_NAME}` tool. Return no prose unless image generation is unavailable."
        ),
        input,
        tools,
        tool_choice: "auto".to_string(),
        parallel_tool_calls: false,
        reasoning: None,
        store: provider.is_azure_responses_endpoint(),
        stream: true,
        include: Vec::new(),
        service_tier: None,
        prompt_cache_key: Some(route_request_id.to_string()),
        text: None,
    })
}

fn routed_model_slug(turn: &crate::praxis::TurnContext) -> String {
    let current_model_supports_images =
        turn.model_info
            .experimental_supported_tools
            .iter()
            .any(|tool| {
                tool == IMAGE_GENERATION_TOOL_NAME || tool == "image_gen" || tool == "imagegen"
            });
    if turn.config.model_provider_id == OPENAI_PROVIDER_ID && current_model_supports_images {
        turn.model_info.slug.clone()
    } else {
        ROUTED_IMAGE_MODEL.to_string()
    }
}

async fn emit_image_generation_failure(
    session: &crate::praxis::Session,
    turn: &crate::praxis::TurnContext,
    call_id: &str,
) {
    session
        .send_event(
            turn,
            EventMsg::ImageGenerationEnd(ImageGenerationEndEvent {
                call_id: call_id.to_string(),
                status: "failed".to_string(),
                revised_prompt: None,
                result: String::new(),
                saved_path: None,
            }),
        )
        .await;
}
