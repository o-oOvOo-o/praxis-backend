use praxis_protocol::models::ResponseItem;

use super::OPAQUE_RESPONSE_ITEM_FORMAT;

pub(super) enum OpaquePromptItemProjection {
    Opaque(praxis_loop::model::PromptItem),
    DecodeResponseItem,
}

pub(super) enum OpaqueResponseItemProjection {
    Restored(ResponseItem),
    NotOpaque,
    InvalidOpaque,
}

pub(super) fn opaque_prompt_item_projection(item: &ResponseItem) -> OpaquePromptItemProjection {
    serde_json::to_string(item).map_or(OpaquePromptItemProjection::DecodeResponseItem, |data| {
        OpaquePromptItemProjection::Opaque(praxis_loop::model::PromptItem::Opaque {
            format: OPAQUE_RESPONSE_ITEM_FORMAT.to_string(),
            data,
        })
    })
}

pub(super) fn response_item_projection_from_opaque_prompt_item(
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
