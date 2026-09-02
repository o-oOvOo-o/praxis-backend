use praxis_protocol::protocol::EventMsg;
use praxis_protocol::protocol::RolloutItem;
use praxis_thread_store_contracts::ContentRef;

use super::decode_item;
use super::encode_item;

#[test]
fn native_payload_round_trips_the_existing_protocol_item() {
    let item = RolloutItem::EventMsg(EventMsg::Commentary);
    let encoded = encode_item(&item).expect("encode rollout item");
    let decoded = decode_item(&encoded).expect("decode rollout item");

    assert_eq!(
        serde_json::to_value(decoded).expect("decoded json"),
        serde_json::to_value(item).expect("source json")
    );
}

#[test]
fn foreign_native_payload_is_not_treated_as_a_rollout_projection() {
    let content = ContentRef::InlineText {
        text: r#"{"schema":"foreign","item":{"type":"commentary"}}"#.to_string(),
    };

    assert!(decode_item(&content).is_none());
}
