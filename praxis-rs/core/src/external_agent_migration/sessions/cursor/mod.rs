mod convert;
mod db;
mod extract;
mod locator;
mod model;

use self::convert::build_record;
use self::db::open_cursor_db;
use self::db::read_cursor_disk_values;
use self::extract::load_workspace_heads;
use self::locator::locate_cursor_paths;
use self::model::composer_data_keys_for_heads;
use self::model::parse_composer_bubble_headers;
use super::provider::ExternalSessionSyncStats;
use super::source::ExternalAgentSource;
use super::store::ExternalSessionStore;
use crate::config::Config;
use std::io;
use tracing::warn;

pub(super) const SOURCE: ExternalAgentSource = ExternalAgentSource::Cursor;

pub(super) async fn sync_sessions_to_store(
    config: &Config,
) -> io::Result<ExternalSessionSyncStats> {
    let Some(paths) = locate_cursor_paths() else {
        return Ok(ExternalSessionSyncStats::default());
    };

    let heads = load_workspace_heads(&paths.workspace_storage, config.cwd.as_path()).await?;
    if heads.is_empty() {
        return Ok(ExternalSessionSyncStats::default());
    }

    let global_pool = open_cursor_db(&paths.global_db).await?;
    let composer_keys = composer_data_keys_for_heads(&heads);
    let composer_values = read_cursor_disk_values(&global_pool, &composer_keys).await?;
    let store = ExternalSessionStore::open(config, SOURCE).await;
    let mut stats = ExternalSessionSyncStats::discovered(heads.len());

    for head in heads {
        let Some(composer_value) = head.raw_composer_data(&composer_values) else {
            stats.skip_one();
            continue;
        };
        let headers = match parse_composer_bubble_headers(composer_value) {
            Ok(headers) => headers,
            Err(err) => {
                warn!(
                    "failed to parse Cursor composerData for {}: {err}",
                    head.external_id()
                );
                stats.skip_one();
                continue;
            }
        };
        if headers.is_empty() {
            stats.skip_one();
            continue;
        }
        let bubble_keys = head.bubble_keys(&headers);
        let bubble_values = read_cursor_disk_values(&global_pool, &bubble_keys).await?;
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
