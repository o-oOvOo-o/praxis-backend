use super::*;
use crate::DirectionalThreadSpawnEdgeStatus;
use crate::runtime::test_support::test_thread_metadata;
use crate::runtime::test_support::unique_temp_dir;
use praxis_protocol::protocol::EventMsg;
use praxis_protocol::protocol::GitInfo;
use praxis_protocol::protocol::SessionMeta;
use praxis_protocol::protocol::SessionMetaLine;
use praxis_protocol::protocol::SessionSource;
use praxis_protocol::protocol::SubAgentSource;
use pretty_assertions::assert_eq;
use std::path::PathBuf;

#[test]
fn sqlite_like_escape_escapes_wildcards_and_escape_char() {
    assert_eq!(sqlite_like_escape(r"C:\a%b_c"), r"C:\\a\%b\_c");
}

#[test]
fn thread_search_fts_query_uses_prefix_tokens() {
    assert_eq!(
        thread_search_fts_query("Legacy Praxis").as_deref(),
        Some("legacy* AND praxis*")
    );
    assert_eq!(thread_search_fts_query("///"), None);
}

#[tokio::test]
async fn list_threads_cwd_filter_matches_descendant_paths() {
    let praxis_home = unique_temp_dir();
    let runtime = StateRuntime::init(praxis_home.clone(), "test-provider".to_string())
        .await
        .expect("state db should initialize");
    let root_id = ThreadId::from_string("00000000-0000-0000-0000-000000000201").expect("root id");
    let child_id = ThreadId::from_string("00000000-0000-0000-0000-000000000202").expect("child id");
    let sibling_id =
        ThreadId::from_string("00000000-0000-0000-0000-000000000203").expect("sibling id");
    let project_root = praxis_home.join("project");

    runtime
        .upsert_thread(&test_thread_metadata(
            &praxis_home,
            root_id,
            project_root.clone(),
        ))
        .await
        .expect("root thread");
    runtime
        .upsert_thread(&test_thread_metadata(
            &praxis_home,
            child_id,
            project_root.join("src"),
        ))
        .await
        .expect("child thread");
    runtime
        .upsert_thread(&test_thread_metadata(
            &praxis_home,
            sibling_id,
            praxis_home.join("project-sibling"),
        ))
        .await
        .expect("sibling thread");

    let cwd = project_root.display().to_string();
    let allowed_sources = vec!["cli".to_string()];
    let page = runtime
        .list_threads(
            10,
            /*anchor*/ None,
            SortKey::UpdatedAt,
            allowed_sources.as_slice(),
            /*source_kinds*/ None,
            /*model_providers*/ None,
            /*archived_only*/ false,
            Some(cwd.as_str()),
            /*search_term*/ None,
        )
        .await
        .expect("list threads");

    assert_eq!(page.items.len(), 2);
    assert!(page.items.iter().any(|item| item.id == root_id));
    assert!(page.items.iter().any(|item| item.id == child_id));
    assert!(!page.items.iter().any(|item| item.id == sibling_id));
}

#[tokio::test]
async fn list_threads_filters_mixed_source_kinds() {
    let praxis_home = unique_temp_dir();
    let runtime = StateRuntime::init(praxis_home.clone(), "test-provider".to_string())
        .await
        .expect("state db should initialize");
    let cli_id =
        ThreadId::from_string("00000000-0000-0000-0000-000000000921").expect("valid thread id");
    let spawn_id =
        ThreadId::from_string("00000000-0000-0000-0000-000000000922").expect("valid thread id");
    let exec_id =
        ThreadId::from_string("00000000-0000-0000-0000-000000000923").expect("valid thread id");

    runtime
        .upsert_thread(&test_thread_metadata(
            &praxis_home,
            cli_id,
            praxis_home.clone(),
        ))
        .await
        .expect("cli thread");

    let mut spawn = test_thread_metadata(&praxis_home, spawn_id, praxis_home.clone());
    spawn.source =
        crate::extract::enum_to_string(&SessionSource::SubAgent(SubAgentSource::ThreadSpawn {
            parent_thread_id: cli_id,
            depth: 1,
            agent_path: None,
            agent_base_name: None,
            agent_title: None,
            agent_display_name: Some("builder".to_string()),
            agent_role: None,
        }));
    runtime.upsert_thread(&spawn).await.expect("spawn thread");

    let mut exec = test_thread_metadata(&praxis_home, exec_id, praxis_home.clone());
    exec.source = crate::extract::enum_to_string(&SessionSource::Exec);
    runtime.upsert_thread(&exec).await.expect("exec thread");

    let source_kinds = [
        crate::ThreadSourceKind::Cli,
        crate::ThreadSourceKind::SubAgentThreadSpawn,
    ];
    let page = runtime
        .list_threads(
            10,
            /*anchor*/ None,
            SortKey::UpdatedAt,
            &[],
            Some(source_kinds.as_slice()),
            /*model_providers*/ None,
            /*archived_only*/ false,
            /*cwd*/ None,
            /*search_term*/ None,
        )
        .await
        .expect("list threads");

    assert!(page.items.iter().any(|item| item.id == cli_id));
    assert!(page.items.iter().any(|item| item.id == spawn_id));
    assert!(!page.items.iter().any(|item| item.id == exec_id));
}

#[tokio::test]
async fn upsert_thread_keeps_creation_memory_mode_for_existing_rows() {
    let praxis_home = unique_temp_dir();
    let runtime = StateRuntime::init(praxis_home.clone(), "test-provider".to_string())
        .await
        .expect("state db should initialize");
    let thread_id =
        ThreadId::from_string("00000000-0000-0000-0000-000000000123").expect("valid thread id");
    let mut metadata = test_thread_metadata(&praxis_home, thread_id, praxis_home.clone());

    runtime
        .upsert_thread_with_creation_memory_mode(&metadata, Some("disabled"))
        .await
        .expect("initial insert should succeed");

    let memory_mode: String = sqlx::query_scalar("SELECT memory_mode FROM threads WHERE id = ?")
        .bind(thread_id.to_string())
        .fetch_one(runtime.pool.as_ref())
        .await
        .expect("memory mode should be readable");
    assert_eq!(memory_mode, "disabled");

    metadata.title = "updated title".to_string();
    runtime
        .upsert_thread(&metadata)
        .await
        .expect("upsert should succeed");

    let memory_mode: String = sqlx::query_scalar("SELECT memory_mode FROM threads WHERE id = ?")
        .bind(thread_id.to_string())
        .fetch_one(runtime.pool.as_ref())
        .await
        .expect("memory mode should remain readable");
    assert_eq!(memory_mode, "disabled");
}

#[tokio::test]
async fn apply_rollout_items_restores_memory_mode_from_session_meta() {
    let praxis_home = unique_temp_dir();
    let runtime = StateRuntime::init(praxis_home.clone(), "test-provider".to_string())
        .await
        .expect("state db should initialize");
    let thread_id =
        ThreadId::from_string("00000000-0000-0000-0000-000000000456").expect("valid thread id");
    let metadata = test_thread_metadata(&praxis_home, thread_id, praxis_home.clone());

    runtime
        .upsert_thread(&metadata)
        .await
        .expect("initial upsert should succeed");

    let builder = ThreadMetadataBuilder::new(
        thread_id,
        metadata.rollout_path.clone(),
        metadata.created_at,
        SessionSource::Cli,
    );
    let items = vec![RolloutItem::SessionMeta(SessionMetaLine {
        meta: SessionMeta {
            id: thread_id,
            forked_from_id: None,
            timestamp: metadata.created_at.to_rfc3339(),
            cwd: PathBuf::new(),
            originator: String::new(),
            cli_version: String::new(),
            source: SessionSource::Cli,
            agent_path: None,
            agent_base_name: None,
            agent_title: None,
            agent_display_name: None,
            agent_role: None,
            model_provider: None,
            base_instructions: None,
            dynamic_tools: None,
            memory_mode: Some("polluted".to_string()),
        },
        git: None,
    })];

    runtime
        .apply_rollout_items(
            &builder, &items, /*new_thread_memory_mode*/ None,
            /*updated_at_override*/ None,
        )
        .await
        .expect("apply_rollout_items should succeed");

    let memory_mode = runtime
        .get_thread_memory_mode(thread_id)
        .await
        .expect("memory mode should load");
    assert_eq!(memory_mode.as_deref(), Some("polluted"));
}

#[tokio::test]
async fn apply_rollout_items_preserves_existing_git_branch_and_fills_missing_git_fields() {
    let praxis_home = unique_temp_dir();
    let runtime = StateRuntime::init(praxis_home.clone(), "test-provider".to_string())
        .await
        .expect("state db should initialize");
    let thread_id =
        ThreadId::from_string("00000000-0000-0000-0000-000000000457").expect("valid thread id");
    let mut metadata = test_thread_metadata(&praxis_home, thread_id, praxis_home.clone());
    metadata.git_branch = Some("sqlite-branch".to_string());

    runtime
        .upsert_thread(&metadata)
        .await
        .expect("initial upsert should succeed");

    let created_at = metadata.created_at.to_rfc3339();
    let builder = ThreadMetadataBuilder::new(
        thread_id,
        metadata.rollout_path.clone(),
        metadata.created_at,
        SessionSource::Cli,
    );
    let items = vec![RolloutItem::SessionMeta(SessionMetaLine {
        meta: SessionMeta {
            id: thread_id,
            forked_from_id: None,
            timestamp: created_at,
            cwd: PathBuf::new(),
            originator: String::new(),
            cli_version: String::new(),
            source: SessionSource::Cli,
            agent_path: None,
            agent_base_name: None,
            agent_title: None,
            agent_display_name: None,
            agent_role: None,
            model_provider: None,
            base_instructions: None,
            dynamic_tools: None,
            memory_mode: None,
        },
        git: Some(GitInfo {
            commit_hash: Some(praxis_git_utils::GitSha::new("rollout-sha")),
            branch: Some("rollout-branch".to_string()),
            repository_url: Some("git@example.com:cunning3d/praxis.git".to_string()),
        }),
    })];

    runtime
        .apply_rollout_items(
            &builder, &items, /*new_thread_memory_mode*/ None,
            /*updated_at_override*/ None,
        )
        .await
        .expect("apply_rollout_items should succeed");

    let persisted = runtime
        .get_thread(thread_id)
        .await
        .expect("thread should load")
        .expect("thread should exist");
    assert_eq!(persisted.git_sha.as_deref(), Some("rollout-sha"));
    assert_eq!(persisted.git_branch.as_deref(), Some("sqlite-branch"));
    assert_eq!(
        persisted.git_origin_url.as_deref(),
        Some("git@example.com:cunning3d/praxis.git")
    );
}

#[tokio::test]
async fn update_thread_git_info_preserves_newer_non_git_metadata() {
    let praxis_home = unique_temp_dir();
    let runtime = StateRuntime::init(praxis_home.clone(), "test-provider".to_string())
        .await
        .expect("state db should initialize");
    let thread_id =
        ThreadId::from_string("00000000-0000-0000-0000-000000000789").expect("valid thread id");
    let metadata = test_thread_metadata(&praxis_home, thread_id, praxis_home.clone());

    runtime
        .upsert_thread(&metadata)
        .await
        .expect("initial upsert should succeed");

    let updated_at = datetime_to_epoch_seconds(
        DateTime::<Utc>::from_timestamp(1_700_000_100, 0).expect("timestamp"),
    );
    sqlx::query(
        "UPDATE threads SET updated_at = ?, tokens_used = ?, first_user_message = ? WHERE id = ?",
    )
    .bind(updated_at)
    .bind(123_i64)
    .bind("newer preview")
    .bind(thread_id.to_string())
    .execute(runtime.pool.as_ref())
    .await
    .expect("concurrent metadata write should succeed");

    let updated = runtime
        .update_thread_git_info(
            thread_id,
            Some(Some("abc123")),
            Some(Some("feature/branch")),
            Some(Some("git@example.com:cunning3d/praxis.git")),
        )
        .await
        .expect("git info update should succeed");
    assert!(updated, "git info update should touch the thread row");

    let persisted = runtime
        .get_thread(thread_id)
        .await
        .expect("thread should load")
        .expect("thread should exist");
    assert_eq!(persisted.tokens_used, 123);
    assert_eq!(
        persisted.first_user_message.as_deref(),
        Some("newer preview")
    );
    assert_eq!(datetime_to_epoch_seconds(persisted.updated_at), updated_at);
    assert_eq!(persisted.git_sha.as_deref(), Some("abc123"));
    assert_eq!(persisted.git_branch.as_deref(), Some("feature/branch"));
    assert_eq!(
        persisted.git_origin_url.as_deref(),
        Some("git@example.com:cunning3d/praxis.git")
    );
}

#[tokio::test]
async fn insert_thread_if_absent_preserves_existing_metadata() {
    let praxis_home = unique_temp_dir();
    let runtime = StateRuntime::init(praxis_home.clone(), "test-provider".to_string())
        .await
        .expect("state db should initialize");
    let thread_id =
        ThreadId::from_string("00000000-0000-0000-0000-000000000791").expect("valid thread id");

    let mut existing = test_thread_metadata(&praxis_home, thread_id, praxis_home.clone());
    existing.tokens_used = 123;
    existing.first_user_message = Some("newer preview".to_string());
    existing.updated_at = DateTime::<Utc>::from_timestamp(1_700_000_100, 0).expect("timestamp");
    runtime
        .upsert_thread(&existing)
        .await
        .expect("initial upsert should succeed");

    let mut fallback = test_thread_metadata(&praxis_home, thread_id, praxis_home.clone());
    fallback.tokens_used = 0;
    fallback.first_user_message = None;
    fallback.updated_at = DateTime::<Utc>::from_timestamp(1_700_000_000, 0).expect("timestamp");

    let inserted = runtime
        .insert_thread_if_absent(&fallback)
        .await
        .expect("insert should succeed");
    assert!(!inserted, "existing rows should not be overwritten");

    let persisted = runtime
        .get_thread(thread_id)
        .await
        .expect("thread should load")
        .expect("thread should exist");
    assert_eq!(persisted.tokens_used, 123);
    assert_eq!(
        persisted.first_user_message.as_deref(),
        Some("newer preview")
    );
    assert_eq!(
        datetime_to_epoch_seconds(persisted.updated_at),
        datetime_to_epoch_seconds(existing.updated_at)
    );
}

#[tokio::test]
async fn update_thread_git_info_can_clear_fields() {
    let praxis_home = unique_temp_dir();
    let runtime = StateRuntime::init(praxis_home.clone(), "test-provider".to_string())
        .await
        .expect("state db should initialize");
    let thread_id =
        ThreadId::from_string("00000000-0000-0000-0000-000000000790").expect("valid thread id");
    let mut metadata = test_thread_metadata(&praxis_home, thread_id, praxis_home.clone());
    metadata.git_sha = Some("abc123".to_string());
    metadata.git_branch = Some("feature/branch".to_string());
    metadata.git_origin_url = Some("git@example.com:cunning3d/praxis.git".to_string());

    runtime
        .upsert_thread(&metadata)
        .await
        .expect("initial upsert should succeed");

    let updated = runtime
        .update_thread_git_info(thread_id, Some(None), Some(None), Some(None))
        .await
        .expect("git info clear should succeed");
    assert!(updated, "git info clear should touch the thread row");

    let persisted = runtime
        .get_thread(thread_id)
        .await
        .expect("thread should load")
        .expect("thread should exist");
    assert_eq!(persisted.git_sha, None);
    assert_eq!(persisted.git_branch, None);
    assert_eq!(persisted.git_origin_url, None);
}

#[tokio::test]
async fn touch_thread_updated_at_updates_only_updated_at() {
    let praxis_home = unique_temp_dir();
    let runtime = StateRuntime::init(praxis_home.clone(), "test-provider".to_string())
        .await
        .expect("state db should initialize");
    let thread_id =
        ThreadId::from_string("00000000-0000-0000-0000-000000000791").expect("valid thread id");
    let mut metadata = test_thread_metadata(&praxis_home, thread_id, praxis_home.clone());
    metadata.title = "original title".to_string();
    metadata.first_user_message = Some("first-user-message".to_string());

    runtime
        .upsert_thread(&metadata)
        .await
        .expect("initial upsert should succeed");

    let touched_at = DateTime::<Utc>::from_timestamp(1_700_001_111, 0).expect("timestamp");
    let touched = runtime
        .touch_thread_updated_at(thread_id, touched_at)
        .await
        .expect("touch should succeed");
    assert!(touched);

    let persisted = runtime
        .get_thread(thread_id)
        .await
        .expect("thread should load")
        .expect("thread should exist");
    assert_eq!(persisted.updated_at, touched_at);
    assert_eq!(persisted.title, "original title");
    assert_eq!(
        persisted.first_user_message.as_deref(),
        Some("first-user-message")
    );
}

#[tokio::test]
async fn apply_rollout_items_uses_override_updated_at_when_provided() {
    let praxis_home = unique_temp_dir();
    let runtime = StateRuntime::init(praxis_home.clone(), "test-provider".to_string())
        .await
        .expect("state db should initialize");
    let thread_id =
        ThreadId::from_string("00000000-0000-0000-0000-000000000792").expect("valid thread id");
    let metadata = test_thread_metadata(&praxis_home, thread_id, praxis_home.clone());

    runtime
        .upsert_thread(&metadata)
        .await
        .expect("initial upsert should succeed");

    let builder = ThreadMetadataBuilder::new(
        thread_id,
        metadata.rollout_path.clone(),
        metadata.created_at,
        SessionSource::Cli,
    );
    let items = vec![RolloutItem::EventMsg(EventMsg::TokenCount(
        praxis_protocol::protocol::TokenCountEvent {
            info: Some(praxis_protocol::protocol::TokenUsageInfo {
                total_token_usage: praxis_protocol::protocol::TokenUsage {
                    input_tokens: 0,
                    cached_input_tokens: 0,
                    cache_reported_input_tokens: 0,
                    output_tokens: 0,
                    reasoning_output_tokens: 0,
                    total_tokens: 321,
                },
                last_token_usage: praxis_protocol::protocol::TokenUsage::default(),
                model_context_window: None,
                model_auto_compact_token_limit: None,
            }),
            rate_limits: None,
        },
    ))];
    let override_updated_at = DateTime::<Utc>::from_timestamp(1_700_001_234, 0).expect("timestamp");

    runtime
        .apply_rollout_items(
            &builder,
            &items,
            /*new_thread_memory_mode*/ None,
            Some(override_updated_at),
        )
        .await
        .expect("apply_rollout_items should succeed");

    let persisted = runtime
        .get_thread(thread_id)
        .await
        .expect("thread should load")
        .expect("thread should exist");
    assert_eq!(persisted.tokens_used, 321);
    assert_eq!(
        persisted
            .token_usage_info
            .as_ref()
            .map(|info| info.total_token_usage.total_tokens),
        Some(321)
    );
    assert_eq!(persisted.updated_at, override_updated_at);
}

#[tokio::test]
async fn thread_spawn_edges_track_directional_status() {
    let praxis_home = unique_temp_dir();
    let runtime = StateRuntime::init(praxis_home, "test-provider".to_string())
        .await
        .expect("state db should initialize");
    let parent_thread_id =
        ThreadId::from_string("00000000-0000-0000-0000-000000000900").expect("valid thread id");
    let child_thread_id =
        ThreadId::from_string("00000000-0000-0000-0000-000000000901").expect("valid thread id");
    let grandchild_thread_id =
        ThreadId::from_string("00000000-0000-0000-0000-000000000902").expect("valid thread id");

    runtime
        .upsert_thread_spawn_edge(
            parent_thread_id,
            child_thread_id,
            DirectionalThreadSpawnEdgeStatus::Open,
        )
        .await
        .expect("child edge insert should succeed");
    runtime
        .upsert_thread_spawn_edge(
            child_thread_id,
            grandchild_thread_id,
            DirectionalThreadSpawnEdgeStatus::Open,
        )
        .await
        .expect("grandchild edge insert should succeed");

    let children = runtime
        .list_thread_spawn_children_with_status(
            parent_thread_id,
            DirectionalThreadSpawnEdgeStatus::Open,
        )
        .await
        .expect("open child list should load");
    assert_eq!(children, vec![child_thread_id]);

    let descendants = runtime
        .list_thread_spawn_descendants_with_status(
            parent_thread_id,
            DirectionalThreadSpawnEdgeStatus::Open,
        )
        .await
        .expect("open descendants should load");
    assert_eq!(descendants, vec![child_thread_id, grandchild_thread_id]);

    runtime
        .set_thread_spawn_edge_status(child_thread_id, DirectionalThreadSpawnEdgeStatus::Closed)
        .await
        .expect("edge close should succeed");

    let open_children = runtime
        .list_thread_spawn_children_with_status(
            parent_thread_id,
            DirectionalThreadSpawnEdgeStatus::Open,
        )
        .await
        .expect("open child list should load");
    assert_eq!(open_children, Vec::<ThreadId>::new());

    let closed_children = runtime
        .list_thread_spawn_children_with_status(
            parent_thread_id,
            DirectionalThreadSpawnEdgeStatus::Closed,
        )
        .await
        .expect("closed child list should load");
    assert_eq!(closed_children, vec![child_thread_id]);

    let closed_descendants = runtime
        .list_thread_spawn_descendants_with_status(
            parent_thread_id,
            DirectionalThreadSpawnEdgeStatus::Closed,
        )
        .await
        .expect("closed descendants should load");
    assert_eq!(closed_descendants, vec![child_thread_id]);

    let open_descendants_from_child = runtime
        .list_thread_spawn_descendants_with_status(
            child_thread_id,
            DirectionalThreadSpawnEdgeStatus::Open,
        )
        .await
        .expect("open descendants from child should load");
    assert_eq!(open_descendants_from_child, vec![grandchild_thread_id]);
}
