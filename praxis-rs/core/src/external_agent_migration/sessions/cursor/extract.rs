use super::db::open_cursor_db;
use super::db::read_kv_value;
use super::model::COMPOSER_DATA_KEY;
use super::model::CursorBubbleHeader;
use super::model::CursorThreadHead;
use super::model::parse_cursor_time;
use super::model::value_i64;
use serde_json::Value;
use std::cmp::Reverse;
use std::collections::HashMap;
use std::io;
use std::path::Path;
use std::path::PathBuf;
use tracing::warn;
use url::Url;

pub(super) async fn load_workspace_heads(
    workspace_storage: &Path,
    fallback_cwd: &Path,
) -> io::Result<Vec<CursorThreadHead>> {
    let mut by_composer: HashMap<String, CursorThreadHead> = HashMap::new();
    for entry in std::fs::read_dir(workspace_storage)? {
        let entry = entry?;
        let workspace_dir = entry.path();
        if !workspace_dir.is_dir() {
            continue;
        }
        let state_db = workspace_dir.join("state.vscdb");
        if !state_db.is_file() {
            continue;
        }
        let cwd = read_workspace_cwd(&workspace_dir).unwrap_or_else(|| fallback_cwd.to_path_buf());
        let pool = match open_cursor_db(&state_db).await {
            Ok(pool) => pool,
            Err(err) => {
                warn!(
                    "failed to open Cursor workspace db {}: {err}",
                    state_db.display()
                );
                continue;
            }
        };
        let composer_data = read_kv_value(&pool, "ItemTable", COMPOSER_DATA_KEY).await?;
        pool.close().await;
        let Some(composer_data) = composer_data else {
            continue;
        };
        let value = match serde_json::from_str::<Value>(&composer_data) {
            Ok(value) => value,
            Err(err) => {
                warn!(
                    "failed to parse Cursor workspace composer data {}: {err}",
                    state_db.display()
                );
                continue;
            }
        };
        for head in parse_workspace_heads(&value, &cwd) {
            by_composer
                .entry(head.composer_id.clone())
                .and_modify(|existing| {
                    if compare_updated_at(&head, existing).is_gt() {
                        *existing = head.clone();
                    }
                })
                .or_insert(head);
        }
    }

    let mut heads = by_composer.into_values().collect::<Vec<_>>();
    heads.sort_by_key(|head| Reverse(head.updated_at.or(head.created_at)));
    Ok(heads)
}

pub(super) fn parse_bubble_headers(value: &Value) -> Vec<CursorBubbleHeader> {
    value
        .get("fullConversationHeadersOnly")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|header| {
            let bubble_id = header
                .get("bubbleId")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())?
                .to_string();
            Some(CursorBubbleHeader {
                bubble_id,
                kind: value_i64(header.get("type")),
            })
        })
        .collect()
}

fn parse_workspace_heads(value: &Value, cwd: &Path) -> Vec<CursorThreadHead> {
    value
        .get("allComposers")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|head| {
            if head
                .get("isArchived")
                .and_then(Value::as_bool)
                .unwrap_or(false)
            {
                return None;
            }
            let composer_id = head
                .get("composerId")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())?
                .to_string();
            let name = head
                .get("name")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned);
            Some(CursorThreadHead {
                composer_id,
                name,
                created_at: parse_cursor_time(head.get("createdAt")),
                updated_at: parse_cursor_time(head.get("lastUpdatedAt")),
                cwd: cwd.to_path_buf(),
            })
        })
        .collect()
}

fn read_workspace_cwd(workspace_dir: &Path) -> Option<PathBuf> {
    let workspace_json = std::fs::read_to_string(workspace_dir.join("workspace.json")).ok()?;
    let value = serde_json::from_str::<Value>(&workspace_json).ok()?;
    let folder = value.get("folder").and_then(Value::as_str)?;
    Url::parse(folder).ok()?.to_file_path().ok()
}

fn compare_updated_at(left: &CursorThreadHead, right: &CursorThreadHead) -> std::cmp::Ordering {
    let left_time = left.updated_at.or(left.created_at);
    let right_time = right.updated_at.or(right.created_at);
    left_time.cmp(&right_time)
}
