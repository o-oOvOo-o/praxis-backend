use super::message_buffer::ResponseMessageBuffer;
use super::opaque;
use super::opaque::OpaqueResponseItemProjection;
use super::prompt_image_encoder;
use super::prompt_text_encoder;
use super::prompt_tool_encoder;

pub(super) fn push_prompt_item(
    buffer: &mut ResponseMessageBuffer,
    item: &praxis_loop::model::PromptItem,
) {
    match item {
        praxis_loop::model::PromptItem::SystemText(text) => {
            prompt_text_encoder::push_system_text(buffer, text);
        }
        praxis_loop::model::PromptItem::UserText(text) => {
            prompt_text_encoder::push_user_text(buffer, text);
        }
        praxis_loop::model::PromptItem::AssistantText(text) => {
            prompt_text_encoder::push_assistant_text(buffer, text);
        }
        praxis_loop::model::PromptItem::ImageUrl(image_url) => {
            prompt_image_encoder::push_image_url(buffer, image_url);
        }
        praxis_loop::model::PromptItem::LocalImagePath(path) => {
            prompt_image_encoder::push_local_image_path(buffer, path);
        }
        praxis_loop::model::PromptItem::ToolCall {
            call_id,
            name,
            arguments,
        } => prompt_tool_encoder::push_tool_call(buffer, call_id, name, arguments),
        praxis_loop::model::PromptItem::ToolResult {
            call_id,
            content,
            status,
        } => prompt_tool_encoder::push_tool_result(buffer, call_id, content, status.is_error()),
        praxis_loop::model::PromptItem::Opaque { format, data } => {
            match opaque::response_item_projection_from_opaque_prompt_item(format, data) {
                OpaqueResponseItemProjection::Restored(item) => buffer.push_item(item),
                OpaqueResponseItemProjection::NotOpaque
                | OpaqueResponseItemProjection::InvalidOpaque => {}
            }
        }
        praxis_loop::model::PromptItem::Skill { .. }
        | praxis_loop::model::PromptItem::Mention { .. } => {}
    }
}
