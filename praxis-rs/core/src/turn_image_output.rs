use std::path::Path;
use std::path::PathBuf;

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use praxis_protocol::items::ImageGenerationItem;
use praxis_protocol::models::DeveloperInstructions;
use praxis_protocol::models::ResponseItem;

use crate::error::PraxisErr;
use crate::error::Result;
use crate::praxis::Session;
use crate::praxis::TurnContext;

const GENERATED_IMAGE_ARTIFACTS_DIR: &str = "generated_images";

pub(crate) fn image_generation_artifact_path(
    praxis_home: &Path,
    session_id: &str,
    call_id: &str,
) -> PathBuf {
    let sanitize = |value: &str| {
        let mut sanitized: String = value
            .chars()
            .map(|ch| {
                if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                    ch
                } else {
                    '_'
                }
            })
            .collect();
        if sanitized.is_empty() {
            sanitized = "generated_image".to_string();
        }
        sanitized
    };

    praxis_home
        .join(GENERATED_IMAGE_ARTIFACTS_DIR)
        .join(sanitize(session_id))
        .join(format!("{}.png", sanitize(call_id)))
}

pub(crate) async fn save_image_generation_result(
    praxis_home: &Path,
    session_id: &str,
    call_id: &str,
    result: &str,
) -> Result<PathBuf> {
    let bytes = BASE64_STANDARD
        .decode(result.trim().as_bytes())
        .map_err(|err| {
            PraxisErr::InvalidRequest(format!("invalid image generation payload: {err}"))
        })?;
    let path = image_generation_artifact_path(praxis_home, session_id, call_id);
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    tokio::fs::write(&path, bytes).await?;
    Ok(path)
}

pub(crate) async fn save_generated_image_for_turn_item(
    sess: &Session,
    turn_context: &TurnContext,
    image_item: &mut ImageGenerationItem,
) {
    let session_id = sess.conversation_id.to_string();
    match save_image_generation_result(
        turn_context.config.praxis_home.as_path(),
        &session_id,
        &image_item.id,
        &image_item.result,
    )
    .await
    {
        Ok(path) => {
            image_item.saved_path = Some(path.to_string_lossy().into_owned());
            record_generated_image_storage_instructions(sess, turn_context, &session_id).await;
        }
        Err(err) => {
            let output_path = image_generation_artifact_path(
                turn_context.config.praxis_home.as_path(),
                &session_id,
                &image_item.id,
            );
            let output_dir = output_path
                .parent()
                .unwrap_or(turn_context.config.praxis_home.as_path());
            tracing::warn!(
                call_id = %image_item.id,
                output_dir = %output_dir.display(),
                "failed to save generated image: {err}"
            );
        }
    }
}

async fn record_generated_image_storage_instructions(
    sess: &Session,
    turn_context: &TurnContext,
    session_id: &str,
) {
    let image_output_path = image_generation_artifact_path(
        turn_context.config.praxis_home.as_path(),
        session_id,
        "<image_id>",
    );
    let image_output_dir = image_output_path
        .parent()
        .unwrap_or(turn_context.config.praxis_home.as_path());
    let message: ResponseItem = DeveloperInstructions::new(format!(
        "Generated images are saved to {} as {} by default.",
        image_output_dir.display(),
        image_output_path.display(),
    ))
    .into();
    let copy_message: ResponseItem = DeveloperInstructions::new(
        "If you need to use a generated image at another path, copy it and leave the original in place unless the user explicitly asks you to delete it."
            .to_string(),
    )
    .into();
    sess.record_conversation_items(turn_context, &[message, copy_message])
        .await;
}

#[cfg(test)]
#[path = "turn_image_output_tests.rs"]
mod tests;
