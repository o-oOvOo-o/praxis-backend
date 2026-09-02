use std::io;

use praxis_protocol::protocol::RolloutItem;
use praxis_thread_store_contracts::ContentRef;
use serde::Deserialize;
use serde::Serialize;

const NATIVE_ROLLOUT_SCHEMA: &str = "praxis.rollout-item.v1";

#[derive(Serialize)]
struct StoredRolloutItemRef<'a> {
    schema: &'static str,
    item: &'a RolloutItem,
}

#[derive(Deserialize)]
struct StoredRolloutItem {
    schema: String,
    item: RolloutItem,
}

pub(crate) fn encode_item(item: &RolloutItem) -> io::Result<ContentRef> {
    Ok(ContentRef::InlineText {
        text: serde_json::to_string(&StoredRolloutItemRef {
            schema: NATIVE_ROLLOUT_SCHEMA,
            item,
        })?,
    })
}

pub(crate) fn decode_item(content: &ContentRef) -> Option<RolloutItem> {
    let ContentRef::InlineText { text } = content else {
        return None;
    };
    let stored: StoredRolloutItem = serde_json::from_str(text).ok()?;
    (stored.schema == NATIVE_ROLLOUT_SCHEMA).then_some(stored.item)
}

#[cfg(test)]
#[path = "native_codec_tests.rs"]
mod tests;
