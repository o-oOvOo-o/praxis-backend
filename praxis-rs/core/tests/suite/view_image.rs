#![cfg(not(target_os = "windows"))]

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use core_test_support::responses;
use core_test_support::responses::ev_assistant_message;
use core_test_support::responses::ev_completed;
use core_test_support::responses::ev_custom_tool_call;
use core_test_support::responses::ev_function_call;
use core_test_support::responses::ev_response_created;
use core_test_support::responses::mount_models_once;
use core_test_support::responses::sse;
use core_test_support::responses::start_mock_server;
use core_test_support::skip_if_no_network;
use core_test_support::test_praxis::TestPraxis;
use core_test_support::test_praxis::test_praxis;
use core_test_support::wait_for_event;
use core_test_support::wait_for_event_with_timeout;
use image::DynamicImage;
use image::GenericImageView;
use image::ImageBuffer;
use image::Rgba;
use image::load_from_memory;
use praxis_exec_server::CreateDirectoryOptions;
use praxis_features::Feature;
use praxis_login::OpenAiAccountAuth;
use praxis_protocol::config_types::ReasoningSummary;
use praxis_protocol::openai_models::ConfigShellToolType;
use praxis_protocol::openai_models::InputModality;
use praxis_protocol::openai_models::ModelInfo;
use praxis_protocol::openai_models::ModelVisibility;
use praxis_protocol::openai_models::ModelsResponse;
use praxis_protocol::openai_models::ReasoningEffort;
use praxis_protocol::openai_models::ReasoningEffortPreset;
use praxis_protocol::openai_models::TruncationPolicyConfig;
use praxis_protocol::protocol::AskForApproval;
use praxis_protocol::protocol::EventMsg;
use praxis_protocol::protocol::Op;
use praxis_protocol::protocol::SandboxPolicy;
use praxis_protocol::user_input::UserInput;
use pretty_assertions::assert_eq;
use serde_json::Value;
use std::io::Cursor;
use std::path::PathBuf;
use tokio::time::Duration;
use wiremock::BodyPrintLimit;
use wiremock::MockServer;
#[cfg(not(debug_assertions))]
use wiremock::ResponseTemplate;
#[cfg(not(debug_assertions))]
use wiremock::matchers::body_string_contains;

fn image_messages(body: &Value) -> Vec<&Value> {
    body.get("input")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter(|item| {
                    item.get("type").and_then(Value::as_str) == Some("message")
                        && item
                            .get("content")
                            .and_then(Value::as_array)
                            .map(|content| {
                                content.iter().any(|span| {
                                    span.get("type").and_then(Value::as_str) == Some("input_image")
                                })
                            })
                            .unwrap_or(false)
                })
                .collect()
        })
        .unwrap_or_default()
}

fn find_image_message(body: &Value) -> Option<&Value> {
    image_messages(body).into_iter().next()
}

fn png_bytes(width: u32, height: u32, rgba: [u8; 4]) -> anyhow::Result<Vec<u8>> {
    let image = ImageBuffer::from_pixel(width, height, Rgba(rgba));
    let mut cursor = Cursor::new(Vec::new());
    DynamicImage::ImageRgba8(image).write_to(&mut cursor, image::ImageFormat::Png)?;
    Ok(cursor.into_inner())
}

async fn create_workspace_directory(test: &TestPraxis, rel_path: &str) -> anyhow::Result<PathBuf> {
    let abs_path = test.config.cwd.join(rel_path)?;
    test.fs()
        .create_directory(&abs_path, CreateDirectoryOptions { recursive: true })
        .await?;
    Ok(abs_path.into_path_buf())
}

async fn write_workspace_file(
    test: &TestPraxis,
    rel_path: &str,
    contents: Vec<u8>,
) -> anyhow::Result<PathBuf> {
    let abs_path = test.config.cwd.join(rel_path)?;
    if let Some(parent) = abs_path.parent() {
        test.fs()
            .create_directory(&parent, CreateDirectoryOptions { recursive: true })
            .await?;
    }
    test.fs().write_file(&abs_path, contents).await?;
    Ok(abs_path.into_path_buf())
}

async fn write_workspace_png(
    test: &TestPraxis,
    rel_path: &str,
    width: u32,
    height: u32,
    rgba: [u8; 4],
) -> anyhow::Result<PathBuf> {
    write_workspace_file(test, rel_path, png_bytes(width, height, rgba)?).await
}

#[path = "view_image/bad_request_replacement.rs"]
mod bad_request_replacement;
#[path = "view_image/detail_and_resize_policy.rs"]
mod detail_and_resize_policy;
#[path = "view_image/image_error_cases.rs"]
mod image_error_cases;
#[path = "view_image/js_repl_image.rs"]
mod js_repl_image;
#[path = "view_image/local_and_tool_success.rs"]
mod local_and_tool_success;
