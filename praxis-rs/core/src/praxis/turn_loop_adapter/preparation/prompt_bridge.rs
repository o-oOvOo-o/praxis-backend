use praxis_protocol::models::ResponseInputItem;
use praxis_protocol::models::ResponseItem;
use praxis_protocol::user_input::UserInput;

use super::super::Session;
use super::super::TurnContext;

mod image_content {
    use std::path::PathBuf;

    use praxis_protocol::models::ContentItem;
    use praxis_protocol::models::ResponseInputItem;
    use praxis_protocol::models::image_close_tag_text;
    use praxis_protocol::models::image_open_tag_text;
    use praxis_protocol::user_input::UserInput;

    pub(in crate::praxis::turn_loop_adapter) fn image_url_content_items(
        image_url: String,
    ) -> Vec<ContentItem> {
        vec![
            ContentItem::InputText {
                text: image_open_tag_text(),
            },
            ContentItem::InputImage { image_url },
            ContentItem::InputText {
                text: image_close_tag_text(),
            },
        ]
    }

    pub(in crate::praxis::turn_loop_adapter) fn local_image_prompt_content_items(
        path: &str,
    ) -> Vec<ContentItem> {
        let path = PathBuf::from(path);
        match ResponseInputItem::from(vec![UserInput::LocalImage { path }]) {
            ResponseInputItem::Message { content, .. } => content,
            _ => Vec::new(),
        }
    }
}
mod message_buffer {
    use praxis_protocol::models::ContentItem;
    use praxis_protocol::models::ResponseItem;

    pub(in crate::praxis::turn_loop_adapter) struct ResponseMessageBuffer {
        pending_role: Option<String>,
        pending_content: Vec<ContentItem>,
        items: Vec<ResponseItem>,
    }

    impl ResponseMessageBuffer {
        pub(in crate::praxis::turn_loop_adapter) fn new() -> Self {
            Self {
                pending_role: None,
                pending_content: Vec::new(),
                items: Vec::new(),
            }
        }

        pub(in crate::praxis::turn_loop_adapter) fn push_content(
            &mut self,
            role: &str,
            content: ContentItem,
        ) {
            if self
                .pending_role
                .as_deref()
                .is_some_and(|pending| pending != role)
            {
                self.flush();
            }
            if self.pending_role.is_none() {
                self.pending_role = Some(role.to_string());
            }
            self.pending_content.push(content);
        }

        pub(in crate::praxis::turn_loop_adapter) fn push_item(&mut self, item: ResponseItem) {
            self.flush();
            self.items.push(item);
        }

        pub(in crate::praxis::turn_loop_adapter) fn finish(mut self) -> Vec<ResponseItem> {
            self.flush();
            self.items
        }

        fn flush(&mut self) {
            let Some(role) = self.pending_role.take() else {
                return;
            };
            if self.pending_content.is_empty() {
                return;
            }
            self.items.push(ResponseItem::Message {
                id: None,
                role,
                content: std::mem::take(&mut self.pending_content),
                end_turn: None,
                phase: None,
            });
        }
    }
}
mod message_decoder {
    use praxis_protocol::models::ContentItem;
    use praxis_protocol::models::is_image_close_tag_text;
    use praxis_protocol::models::is_image_open_tag_text;

    use super::prompt_text_decoder;

    pub(in crate::praxis::turn_loop_adapter) fn prompt_items_from_message(
        role: &str,
        content: &[ContentItem],
    ) -> Vec<praxis_loop::model::PromptItem> {
        let mut prompt_items = Vec::new();
        for item in content {
            message_content_projection(role, item).append_to(&mut prompt_items);
        }
        prompt_items
    }

    enum MessageContentProjection {
        Include(praxis_loop::model::PromptItem),
        WrapperOnly,
    }

    impl MessageContentProjection {
        fn append_to(self, prompt_items: &mut Vec<praxis_loop::model::PromptItem>) {
            match self {
                Self::Include(item) => prompt_items.push(item),
                Self::WrapperOnly => {}
            }
        }
    }

    fn message_content_projection(role: &str, item: &ContentItem) -> MessageContentProjection {
        match item {
            ContentItem::InputText { text } | ContentItem::OutputText { text } => {
                if is_image_open_tag_text(text) || is_image_close_tag_text(text) {
                    MessageContentProjection::WrapperOnly
                } else {
                    MessageContentProjection::Include(
                        prompt_text_decoder::prompt_text_item_from_role(role, text.clone()),
                    )
                }
            }
            ContentItem::InputImage { image_url } => MessageContentProjection::Include(
                praxis_loop::model::PromptItem::ImageUrl(image_url.clone()),
            ),
        }
    }
}
mod opaque {
    use praxis_protocol::models::ResponseItem;

    use super::OPAQUE_RESPONSE_ITEM_FORMAT;

    pub(in crate::praxis::turn_loop_adapter) enum OpaquePromptItemProjection {
        Opaque(praxis_loop::model::PromptItem),
        DecodeResponseItem,
    }

    pub(in crate::praxis::turn_loop_adapter) enum OpaqueResponseItemProjection {
        Restored(ResponseItem),
        NotOpaque,
        InvalidOpaque,
    }

    pub(in crate::praxis::turn_loop_adapter) fn opaque_prompt_item_projection(
        item: &ResponseItem,
    ) -> OpaquePromptItemProjection {
        serde_json::to_string(item).map_or(OpaquePromptItemProjection::DecodeResponseItem, |data| {
            OpaquePromptItemProjection::Opaque(praxis_loop::model::PromptItem::Opaque {
                format: OPAQUE_RESPONSE_ITEM_FORMAT.to_string(),
                data,
            })
        })
    }

    pub(in crate::praxis::turn_loop_adapter) fn response_item_projection_from_opaque_prompt_item(
        format: &str,
        data: &str,
    ) -> OpaqueResponseItemProjection {
        if format != OPAQUE_RESPONSE_ITEM_FORMAT {
            return OpaqueResponseItemProjection::NotOpaque;
        }
        serde_json::from_str::<ResponseItem>(data).map_or(
            OpaqueResponseItemProjection::InvalidOpaque,
            OpaqueResponseItemProjection::Restored,
        )
    }
}
mod prompt_image_encoder {
    use super::image_content::image_url_content_items;
    use super::image_content::local_image_prompt_content_items;
    use super::message_buffer::ResponseMessageBuffer;

    pub(in crate::praxis::turn_loop_adapter) fn push_image_url(
        buffer: &mut ResponseMessageBuffer,
        image_url: &str,
    ) {
        for content in image_url_content_items(image_url.to_owned()) {
            buffer.push_content("user", content);
        }
    }

    pub(in crate::praxis::turn_loop_adapter) fn push_local_image_path(
        buffer: &mut ResponseMessageBuffer,
        path: &str,
    ) {
        for content in local_image_prompt_content_items(path) {
            buffer.push_content("user", content);
        }
    }
}
mod prompt_item_encoder {
    use super::message_buffer::ResponseMessageBuffer;
    use super::opaque;
    use super::opaque::OpaqueResponseItemProjection;
    use super::prompt_image_encoder;
    use super::prompt_text_encoder;
    use super::prompt_tool_encoder;

    pub(in crate::praxis::turn_loop_adapter) fn push_prompt_item(
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
}
mod prompt_text_decoder {
    pub(in crate::praxis::turn_loop_adapter) fn prompt_text_item_from_role(
        role: &str,
        text: String,
    ) -> praxis_loop::model::PromptItem {
        match role {
            "user" => praxis_loop::model::PromptItem::UserText(text),
            "assistant" => praxis_loop::model::PromptItem::AssistantText(text),
            _ => praxis_loop::model::PromptItem::SystemText(text),
        }
    }
}
mod prompt_text_encoder {
    use praxis_protocol::models::ContentItem;

    use super::message_buffer::ResponseMessageBuffer;

    pub(in crate::praxis::turn_loop_adapter) fn push_system_text(
        buffer: &mut ResponseMessageBuffer,
        text: &str,
    ) {
        push_text(
            buffer,
            "system",
            ContentItem::InputText { text: text.into() },
        );
    }

    pub(in crate::praxis::turn_loop_adapter) fn push_user_text(
        buffer: &mut ResponseMessageBuffer,
        text: &str,
    ) {
        push_text(buffer, "user", ContentItem::InputText { text: text.into() });
    }

    pub(in crate::praxis::turn_loop_adapter) fn push_assistant_text(
        buffer: &mut ResponseMessageBuffer,
        text: &str,
    ) {
        push_text(
            buffer,
            "assistant",
            ContentItem::OutputText { text: text.into() },
        );
    }

    fn push_text(buffer: &mut ResponseMessageBuffer, role: &str, content: ContentItem) {
        buffer.push_content(role, content);
    }
}
mod prompt_tool_encoder {
    use praxis_protocol::models::FunctionCallOutputPayload;
    use praxis_protocol::models::ResponseItem;

    use super::message_buffer::ResponseMessageBuffer;

    pub(in crate::praxis::turn_loop_adapter) fn push_tool_call(
        buffer: &mut ResponseMessageBuffer,
        call_id: &str,
        name: &str,
        arguments: &str,
    ) {
        buffer.push_item(ResponseItem::FunctionCall {
            id: None,
            provider_metadata: None,
            name: name.to_string(),
            namespace: None,
            arguments: arguments.to_string(),
            call_id: call_id.to_string(),
        });
    }

    pub(in crate::praxis::turn_loop_adapter) fn push_tool_result(
        buffer: &mut ResponseMessageBuffer,
        call_id: &str,
        content: &str,
        is_error: bool,
    ) {
        let mut output = FunctionCallOutputPayload::from_text(content.to_string());
        output.success = Some(!is_error);
        buffer.push_item(ResponseItem::FunctionCallOutput {
            call_id: call_id.to_string(),
            output,
        });
    }
}
mod response_decoder {
    use praxis_protocol::models::ResponseItem;

    use super::message_decoder;
    use super::opaque;
    use super::opaque::OpaquePromptItemProjection;
    use super::tool_decoder;

    pub(in crate::praxis::turn_loop_adapter) fn prompt_items_from_response_items(
        items: &[ResponseItem],
    ) -> Vec<praxis_loop::model::PromptItem> {
        items.iter().flat_map(lossless_prompt_item).collect()
    }

    fn lossless_prompt_item(item: &ResponseItem) -> Vec<praxis_loop::model::PromptItem> {
        match opaque::opaque_prompt_item_projection(item) {
            OpaquePromptItemProjection::Opaque(opaque) => vec![opaque],
            OpaquePromptItemProjection::DecodeResponseItem => decoded_prompt_items(item),
        }
    }

    fn decoded_prompt_items(item: &ResponseItem) -> Vec<praxis_loop::model::PromptItem> {
        match item {
            ResponseItem::Message { role, content, .. } => {
                message_decoder::prompt_items_from_message(role.as_str(), content)
            }
            _ => tool_decoder::prompt_items_from_tool_item(item),
        }
    }
}
mod response_encoder {
    use praxis_protocol::models::ResponseItem;

    use super::message_buffer::ResponseMessageBuffer;
    use super::prompt_item_encoder;

    pub(in crate::praxis::turn_loop_adapter) fn response_items_from_prompt_items(
        prompt_items: &[praxis_loop::model::PromptItem],
    ) -> Vec<ResponseItem> {
        let mut buffer = ResponseMessageBuffer::new();

        for item in prompt_items {
            prompt_item_encoder::push_prompt_item(&mut buffer, item);
        }

        buffer.finish()
    }
}
mod tool_decoder {
    use praxis_loop::tool::ToolResultStatus;
    use praxis_protocol::models::ResponseItem;

    pub(in crate::praxis::turn_loop_adapter) fn prompt_items_from_tool_item(
        item: &ResponseItem,
    ) -> Vec<praxis_loop::model::PromptItem> {
        match item {
            ResponseItem::FunctionCall {
                name,
                arguments,
                call_id,
                ..
            }
            | ResponseItem::CustomToolCall {
                name,
                input: arguments,
                call_id,
                ..
            } => vec![praxis_loop::model::PromptItem::ToolCall {
                call_id: call_id.clone(),
                name: name.clone(),
                arguments: arguments.clone(),
            }],
            ResponseItem::FunctionCallOutput { call_id, output }
            | ResponseItem::CustomToolCallOutput {
                call_id, output, ..
            } => output.body.to_text().map_or_else(Vec::new, |content| {
                vec![praxis_loop::model::PromptItem::ToolResult {
                    call_id: call_id.clone(),
                    content,
                    status: ToolResultStatus::from_success_flag(output.success != Some(false)),
                }]
            }),
            _ => Vec::new(),
        }
    }
}

pub(in crate::praxis::turn_loop_adapter) use response_decoder::prompt_items_from_response_items;
pub(in crate::praxis::turn_loop_adapter) use response_encoder::response_items_from_prompt_items;

pub(in crate::praxis::turn_loop_adapter) const OPAQUE_RESPONSE_ITEM_FORMAT: &str =
    "praxis.response_item.v1";

pub(in crate::praxis::turn_loop_adapter) async fn initial_prompt_items_from_session_history(
    sess: &Session,
    turn_context: &TurnContext,
) -> Vec<praxis_loop::model::PromptItem> {
    let items = sess
        .clone_history()
        .await
        .for_prompt(&turn_context.model_info.input_modalities);
    prompt_items_from_response_items(&items)
}

pub(in crate::praxis::turn_loop_adapter) fn input_to_turn_input(
    input: &[UserInput],
) -> praxis_loop::TurnInput {
    if input.is_empty() {
        return praxis_loop::TurnInput::default();
    }
    let response_input_item = ResponseInputItem::from(input.to_vec());
    let response_item = ResponseItem::from(response_input_item);
    let prompt_items = match opaque::opaque_prompt_item_projection(&response_item) {
        opaque::OpaquePromptItemProjection::Opaque(item) => vec![item],
        opaque::OpaquePromptItemProjection::DecodeResponseItem => {
            prompt_items_from_response_items(std::slice::from_ref(&response_item))
        }
    };
    praxis_loop::TurnInput::from_prompt_items(prompt_items)
}
