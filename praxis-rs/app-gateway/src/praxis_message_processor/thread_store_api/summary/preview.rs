use praxis_protocol::models::ResponseItem;
use praxis_protocol::protocol::EventMsg;

pub(super) fn rollout_preview_from_summary_values(head: &[serde_json::Value]) -> Option<String> {
    head.iter()
        .find_map(thread_preview_from_summary_value)
        .map(praxis_state::thread_preview::ThreadUserPreview::into_display_text)
}

fn thread_preview_from_summary_value(
    value: &serde_json::Value,
) -> Option<praxis_state::thread_preview::ThreadUserPreview> {
    serde_json::from_value::<ResponseItem>(value.clone())
        .ok()
        .and_then(|item| praxis_state::thread_preview::response_item_preview(&item))
        .or_else(|| {
            serde_json::from_value::<EventMsg>(value.clone())
                .ok()
                .and_then(|event| praxis_state::thread_preview::event_msg_preview(&event))
        })
}
