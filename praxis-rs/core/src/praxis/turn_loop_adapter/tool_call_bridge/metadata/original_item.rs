use std::collections::BTreeMap;

use praxis_loop::tool::ToolCall as LoopToolCall;
use praxis_protocol::models::ResponseItem;

pub(in crate::praxis::turn_loop_adapter) enum OriginalResponseItemProjection {
    Restored(ResponseItem),
    Reconstruct,
}

const META_ORIGINAL_RESPONSE_ITEM: &str = "praxis.response_item";

pub(in crate::praxis::turn_loop_adapter) fn from_source_item(
    source_item: Option<&ResponseItem>,
) -> BTreeMap<String, String> {
    let mut metadata = BTreeMap::new();
    if let Some(item) = source_item
        && let Ok(serialized) = serde_json::to_string(item)
    {
        metadata.insert(META_ORIGINAL_RESPONSE_ITEM.to_string(), serialized);
    }
    metadata
}

pub(in crate::praxis::turn_loop_adapter) fn original_response_item_projection(
    call: &LoopToolCall,
) -> OriginalResponseItemProjection {
    let Some(value) = call.metadata.get(META_ORIGINAL_RESPONSE_ITEM) else {
        return OriginalResponseItemProjection::Reconstruct;
    };
    serde_json::from_str(value).map_or(
        OriginalResponseItemProjection::Reconstruct,
        OriginalResponseItemProjection::Restored,
    )
}
