use async_trait::async_trait;
use praxis_loop::tool::ToolEffects;
use praxis_protocol::models::FunctionCallOutputBody;
use praxis_protocol::models::FunctionCallOutputContentItem;
use praxis_protocol::models::FunctionCallOutputPayload;
use praxis_protocol::models::ImageDetail;
use praxis_protocol::models::ResponseInputItem;
use praxis_protocol::openai_models::InputModality;
use praxis_utils_absolute_path::AbsolutePathBuf;
use praxis_utils_image::PromptImageMode;
use praxis_utils_image::load_for_prompt_bytes;
use serde::Deserialize;

use crate::function_tool::FunctionCallError;
use crate::original_image_detail::can_request_original_image_detail;
use crate::tools::context::ToolInvocation;
use crate::tools::context::ToolOutput;
use crate::tools::context::ToolPayload;
use crate::tools::handlers::parse_arguments;
use crate::tools::registry::ToolHandler;
use crate::tools::registry::ToolKind;
use crate::tools::registry::ToolPreparation;
use praxis_protocol::protocol::EventMsg;
use praxis_protocol::protocol::ViewImageToolCallEvent;

pub struct ViewImageHandler;

const VIEW_IMAGE_UNSUPPORTED_MESSAGE: &str =
    "view_image is not allowed because you do not support image inputs";

#[derive(Deserialize)]
struct ViewImageArgs {
    path: String,
    detail: Option<String>,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum ViewImageDetail {
    Original,
}

struct PreparedViewImage {
    abs_path: AbsolutePathBuf,
    detail: Option<ViewImageDetail>,
}

#[async_trait]
impl ToolHandler for ViewImageHandler {
    type Output = ViewImageOutput;

    fn kind(&self) -> ToolKind {
        ToolKind::Function
    }

    async fn prepare(
        &self,
        invocation: &ToolInvocation,
    ) -> Result<ToolPreparation, FunctionCallError> {
        let prepared = prepare_view_image(invocation)?;
        Ok(ToolPreparation::new(ToolEffects::read(
            crate::tools::effects::filesystem_effect_key(prepared.abs_path.as_path()),
        ))
        .with_payload(prepared))
    }

    async fn handle(&self, invocation: ToolInvocation) -> Result<Self::Output, FunctionCallError> {
        let prepared = prepare_view_image(&invocation)?;
        execute_view_image(invocation, prepared).await
    }

    async fn handle_prepared(
        &self,
        invocation: ToolInvocation,
        mut preparation: ToolPreparation,
    ) -> Result<Self::Output, FunctionCallError> {
        let prepared = preparation
            .take_payload::<PreparedViewImage>()
            .ok_or_else(|| {
                FunctionCallError::Fatal("view_image prepared state type mismatch".to_string())
            })?;
        execute_view_image(invocation, prepared).await
    }
}

fn prepare_view_image(invocation: &ToolInvocation) -> Result<PreparedViewImage, FunctionCallError> {
    if !invocation
        .turn
        .model_info
        .input_modalities
        .contains(&InputModality::Image)
    {
        return Err(FunctionCallError::RespondToModel(
            VIEW_IMAGE_UNSUPPORTED_MESSAGE.to_string(),
        ));
    }
    let ToolPayload::Function { arguments } = &invocation.payload else {
        return Err(FunctionCallError::RespondToModel(
            "view_image handler received unsupported payload".to_string(),
        ));
    };
    let args: ViewImageArgs = parse_arguments(arguments)?;
    let detail = match args.detail.as_deref() {
        None => None,
        Some("original") => Some(ViewImageDetail::Original),
        Some(detail) => {
            return Err(FunctionCallError::RespondToModel(format!(
                "view_image.detail only supports `original`; omit `detail` for default resized behavior, got `{detail}`"
            )));
        }
    };
    let abs_path = AbsolutePathBuf::try_from(invocation.turn.resolve_path(Some(args.path)))
        .map_err(|error| {
            FunctionCallError::RespondToModel(format!("unable to resolve image path: {error}"))
        })?;
    Ok(PreparedViewImage { abs_path, detail })
}

async fn execute_view_image(
    invocation: ToolInvocation,
    prepared: PreparedViewImage,
) -> Result<ViewImageOutput, FunctionCallError> {
    let ToolInvocation {
        session,
        turn,
        call_id,
        ..
    } = invocation;
    let PreparedViewImage { abs_path, detail } = prepared;
    crate::tools::effects::record_filesystem_read(abs_path.as_path());

    let metadata = turn
        .environment
        .get_filesystem()
        .get_metadata(&abs_path)
        .await
        .map_err(|error| {
            FunctionCallError::RespondToModel(format!(
                "unable to locate image at `{}`: {error}",
                abs_path.display()
            ))
        })?;

    if !metadata.is_file {
        return Err(FunctionCallError::RespondToModel(format!(
            "image path `{}` is not a file",
            abs_path.display()
        )));
    }
    let file_bytes = turn
        .environment
        .get_filesystem()
        .read_file(&abs_path)
        .await
        .map_err(|error| {
            FunctionCallError::RespondToModel(format!(
                "unable to read image at `{}`: {error}",
                abs_path.display()
            ))
        })?;
    let event_path = abs_path.to_path_buf();

    let can_request_original_detail =
        can_request_original_image_detail(turn.features.get(), &turn.model_info);
    let use_original_detail =
        can_request_original_detail && matches!(detail, Some(ViewImageDetail::Original));
    let image_mode = if use_original_detail {
        PromptImageMode::Original
    } else {
        PromptImageMode::ResizeToFit
    };
    let image_detail = use_original_detail.then_some(ImageDetail::Original);

    let image =
        load_for_prompt_bytes(abs_path.as_path(), file_bytes, image_mode).map_err(|error| {
            FunctionCallError::RespondToModel(format!(
                "unable to process image at `{}`: {error}",
                abs_path.display()
            ))
        })?;
    let image_url = image.into_data_url();

    session
        .send_event(
            turn.as_ref(),
            EventMsg::ViewImageToolCall(ViewImageToolCallEvent {
                call_id,
                path: event_path,
            }),
        )
        .await;

    Ok(ViewImageOutput {
        image_url,
        image_detail,
    })
}

pub struct ViewImageOutput {
    image_url: String,
    image_detail: Option<ImageDetail>,
}

impl ToolOutput for ViewImageOutput {
    fn log_preview(&self) -> String {
        self.image_url.clone()
    }

    fn success_for_logging(&self) -> bool {
        true
    }

    fn to_response_item(&self, call_id: &str, _payload: &ToolPayload) -> ResponseInputItem {
        let body =
            FunctionCallOutputBody::ContentItems(vec![FunctionCallOutputContentItem::InputImage {
                image_url: self.image_url.clone(),
                detail: self.image_detail,
            }]);
        let output = FunctionCallOutputPayload {
            body,
            success: Some(true),
        };

        ResponseInputItem::FunctionCallOutput {
            call_id: call_id.to_string(),
            output,
        }
    }

    fn code_mode_result(&self, _payload: &ToolPayload) -> serde_json::Value {
        serde_json::json!({
            "image_url": self.image_url,
            "detail": self.image_detail
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use serde_json::json;

    #[test]
    fn code_mode_result_returns_image_url_object() {
        let output = ViewImageOutput {
            image_url: "data:image/png;base64,AAA".to_string(),
            image_detail: None,
        };

        let result = output.code_mode_result(&ToolPayload::Function {
            arguments: "{}".to_string(),
        });

        assert_eq!(
            result,
            json!({
                "image_url": "data:image/png;base64,AAA",
                "detail": null,
            })
        );
    }
}
