use super::convert::build_record;
use super::db::open_cursor_db;
use super::db::read_kv_values;
use super::extract::load_workspace_heads;
use super::extract::parse_bubble_headers;
use super::locator::locate_cursor_paths;
use super::super::ExternalAgentSessionProvider;
use super::super::ExternalAgentSource;
use super::super::ExternalSessionStore;
use super::super::ExternalSessionSyncContext;
use super::super::ExternalSessionSyncStats;
use async_trait::async_trait;
use serde_json::Value;
use std::io;
use tracing::warn;

pub struct CursorSessionProvider;

#[async_trait]
impl ExternalAgentSessionProvider for CursorSessionProvider {
    fn source(&self) -> ExternalAgentSource {
        ExternalAgentSource::Cursor
    }

    async fn sync_to_store(
        &self,
        ctx: ExternalSessionSyncContext<'_>,
    ) -> io::Result<ExternalSessionSyncStats> {
        let Some(paths) = locate_cursor_paths() else {
            return Ok(ExternalSessionSyncStats::default());
        };

        let heads =
            load_workspace_heads(&paths.workspace_storage, ctx.config.cwd.as_path()).await?;
        if heads.is_empty() {
            return Ok(ExternalSessionSyncStats::default());
        }

        let global_pool = open_cursor_db(&paths.global_db).await?;
        let composer_keys = heads
            .iter()
            .map(|head| format!("composerData:{}", head.composer_id))
            .collect::<Vec<_>>();
        let composer_values = read_kv_values(&global_pool, "cursorDiskKV", &composer_keys).await?;
        let store = ExternalSessionStore::open(ctx.config, self.source()).await;
        let mut stats = ExternalSessionSyncStats::discovered(heads.len());

        for head in heads {
            let composer_key = format!("composerData:{}", head.composer_id);
            let Some(composer_value) = composer_values.get(&composer_key) else {
                stats.skip_one();
                continue;
            };
            let composer_json = match serde_json::from_str::<Value>(composer_value) {
                Ok(value) => value,
                Err(err) => {
                    warn!(
                        "failed to parse Cursor composerData for {}: {err}",
                        head.composer_id
                    );
                    stats.skip_one();
                    continue;
                }
            };
            let headers = parse_bubble_headers(&composer_json);
            if headers.is_empty() {
                stats.skip_one();
                continue;
            }
            let bubble_keys = headers
                .iter()
                .map(|header| format!("bubbleId:{}:{}", head.composer_id, header.bubble_id))
                .collect::<Vec<_>>();
            let bubble_values = read_kv_values(&global_pool, "cursorDiskKV", &bubble_keys).await?;
            let Some(record) = build_record(&head, &headers, &bubble_values) else {
                stats.skip_one();
                continue;
            };
            store.persist(&record).await?;
            stats.import_one();
        }

        global_pool.close().await;
        Ok(stats)
    }
}
