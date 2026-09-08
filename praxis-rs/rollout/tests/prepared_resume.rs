use praxis_protocol::ThreadId;
use praxis_protocol::protocol::{RolloutItem, RolloutLine, SessionMeta, SessionMetaLine};
use praxis_rollout::recorder::PreparedRolloutResume;
use praxis_rollout::{
    EventPersistenceMode, RolloutConfig, RolloutRecorder, RolloutRecorderParams,
    ThreadHistoryReader,
};
use std::path::{Path, PathBuf};

fn config(home: &Path) -> RolloutConfig {
    RolloutConfig {
        praxis_home: home.into(),
        sqlite_home: home.into(),
        cwd: home.into(),
        model_provider_id: "test".into(),
        generate_memories: false,
    }
}

fn message() -> RolloutItem {
    RolloutItem::ResponseItem(
        serde_json::from_value(serde_json::json!({
            "type":"message", "role":"user", "content":[{"type":"input_text", "text":"fixture"}]
        }))
        .expect("message fixture"),
    )
}

#[tokio::test]
async fn native_thread_remains_discoverable_without_legacy_locator() {
    let (home, path) = fixture().await;
    let history = ThreadHistoryReader::from_praxis_home(home.path().to_path_buf())
        .read_initial_history(&path)
        .await
        .expect("history");
    let praxis_protocol::protocol::InitialHistory::Resumed(history) = history else {
        panic!("resumed history");
    };
    let id = history.conversation_id;
    std::fs::remove_file(&path).expect("remove fixture locator");
    std::fs::write(
        home.path().join("thread-store/rollout-import-v3.complete"),
        b"fixture",
    )
    .expect("migration complete");
    let cfg = config(home.path());
    let store = praxis_rollout::ThreadStore::open(&cfg).await;
    let found = store
        .find_rollout_path(id, Some(false))
        .await
        .expect("resolve native thread");
    assert!(
        found.is_some(),
        "native journal must not disappear with its locator"
    );
    let page = store
        .list_threads(praxis_rollout::ListThreadsQuery {
            page_size: 20,
            cursor: None,
            sort_key: praxis_rollout::list::ThreadSortKey::UpdatedAt,
            model_providers: None,
            source_kinds: None,
            archived: false,
            cwd: None,
            search_term: None,
            fallback_provider: "test".into(),
        })
        .await
        .expect("list native threads");
    assert_eq!(
        page.items
            .iter()
            .map(|item| (item.conversation_id, Some(item.path.clone())))
            .collect::<Vec<_>>(),
        vec![(id, found.clone())]
    );
    let resolved = found.expect("native locator address");
    assert!(!resolved.exists(), "discovery must not rewrite history");
    assert_eq!(
        store
            .read_thread_summary(id, Some(false), "test")
            .await
            .expect("summary")
            .expect("native summary")
            .conversation_id,
        id
    );
    assert!(
        store
            .find_rollout_path(id, Some(true))
            .await
            .expect("archived lookup")
            .is_none()
    );
    assert!(
        PreparedRolloutResume::open(home.path(), &resolved)
            .await
            .expect("resume without locator")
            .is_some()
    );
}

async fn fixture() -> (tempfile::TempDir, PathBuf) {
    let home = tempfile::tempdir().expect("temporary home");
    let id = ThreadId::new();
    let path = home
        .path()
        .join(format!("rollout-2026-01-01T00-00-00-{id}.jsonl"));
    let items = [
        RolloutItem::SessionMeta(SessionMetaLine {
            meta: SessionMeta {
                id,
                cwd: home.path().into(),
                ..Default::default()
            },
            git: None,
        }),
        message(),
    ];
    let lines = items
        .into_iter()
        .map(|item| {
            serde_json::to_string(&RolloutLine {
                timestamp: "2026-01-01T00:00:00Z".into(),
                item,
            })
            .expect("encode line")
        })
        .collect::<Vec<_>>()
        .join("\n");
    std::fs::write(&path, format!("{lines}\n")).expect("write fixture");
    let recorder = RolloutRecorder::new(
        &config(home.path()),
        RolloutRecorderParams::resume(path.clone(), EventPersistenceMode::Extended),
        None,
        None,
    )
    .await
    .expect("import fixture");
    recorder.shutdown().await.expect("shutdown fixture");
    drop(recorder);
    tokio::task::yield_now().await;
    (home, path)
}

#[tokio::test]
async fn recovered_writer_transfers_to_recorder_and_preserves_history_on_append() {
    let (home, path) = fixture().await;
    let (history, writer) = PreparedRolloutResume::open(home.path(), &path)
        .await
        .expect("prepare")
        .expect("native thread")
        .into_parts();
    let mut expected = history.get_rollout_items();
    assert_eq!(expected.len(), 2);
    let recorder = RolloutRecorder::new_with_prepared_writer(
        &config(home.path()),
        RolloutRecorderParams::resume(path.clone(), EventPersistenceMode::Extended),
        None,
        None,
        Some(writer),
    )
    .await
    .expect("reuse already locked writer without reopening journal");
    let item = message();
    recorder
        .record_items(&[item.clone()])
        .await
        .expect("append after transfer");
    recorder.flush().await.expect("flush append");
    expected.push(item);
    let actual = ThreadHistoryReader::from_praxis_home(home.path().to_path_buf())
        .read_items(&path)
        .await
        .expect("read while writer remains open");
    assert_eq!(
        serde_json::to_value(actual).unwrap(),
        serde_json::to_value(expected).unwrap()
    );
    recorder.shutdown().await.expect("shutdown recorder");
}

#[tokio::test]
async fn different_path_rejects_and_releases_prepared_writer() {
    let (home, path) = fixture().await;
    let (_, writer) = PreparedRolloutResume::open(home.path(), &path)
        .await
        .expect("prepare")
        .expect("native thread")
        .into_parts();
    assert!(
        RolloutRecorder::new_with_prepared_writer(
            &config(home.path()),
            RolloutRecorderParams::resume(
                home.path().join("other.jsonl"),
                EventPersistenceMode::Extended
            ),
            None,
            None,
            Some(writer)
        )
        .await
        .is_err()
    );
    assert!(
        PreparedRolloutResume::open(home.path(), &path)
            .await
            .expect("reopen after rejection")
            .is_some()
    );
}

#[tokio::test]
async fn absent_native_thread_is_not_created_by_preparation() {
    let home = tempfile::tempdir().expect("temporary home");
    let path = home.path().join(format!(
        "rollout-2026-01-01T00-00-00-{}.jsonl",
        ThreadId::new()
    ));
    assert!(
        PreparedRolloutResume::open(home.path(), &path)
            .await
            .expect("absent native thread")
            .is_none()
    );
    assert!(!path.exists());
}
