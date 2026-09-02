use praxis_protocol::ThreadId;
use praxis_protocol::protocol::EventMsg;
use praxis_protocol::protocol::RolloutItem;

use super::ThreadHistoryReader;
use crate::thread_store::native_codec;

#[tokio::test]
async fn native_journal_wins_over_a_broken_compatibility_projection() {
    let home = tempfile::tempdir().expect("temporary Praxis home");
    let thread_id = ThreadId::new();
    let native_thread_id =
        praxis_thread_store_contracts::ThreadId::parse(thread_id.to_string().as_str())
            .expect("native thread id");
    let native_store = praxis_thread_store::ThreadStore::from_praxis_home(home.path());
    let native_thread = native_store
        .open_thread(native_thread_id)
        .await
        .expect("open native thread");
    native_thread
        .ensure_created("test", home.path().display().to_string(), None)
        .await
        .expect("create native thread");
    let item = RolloutItem::EventMsg(EventMsg::Commentary);
    native_thread
        .execute(
            praxis_thread_store_contracts::ThreadActor::Runtime,
            None,
            praxis_thread_store_contracts::ThreadCommand::RecordNativeAgentEvent {
                agent_sequence: 1,
                event_id: "test:1".to_string(),
                turn_id: None,
                route: praxis_thread_store_contracts::AgentEventRoute::Transcript,
                payload: native_codec::encode_item(&item).expect("encode rollout item"),
            },
            praxis_thread_store::CommitMode::Durable,
        )
        .await
        .expect("record native event");

    let rollout_path = home
        .path()
        .join("sessions")
        .join(format!("rollout-2026-01-01T00-00-00-{thread_id}.jsonl"));
    std::fs::create_dir_all(rollout_path.parent().expect("rollout parent"))
        .expect("create rollout parent");
    std::fs::write(&rollout_path, b"broken projection\n").expect("write broken projection");
    let reader = ThreadHistoryReader::from_praxis_home(home.path().to_path_buf());
    let items = reader
        .read_items(&rollout_path)
        .await
        .expect("read native rollout");

    assert_eq!(
        serde_json::to_value(items).expect("actual items"),
        serde_json::to_value(vec![item]).expect("expected items")
    );
}
