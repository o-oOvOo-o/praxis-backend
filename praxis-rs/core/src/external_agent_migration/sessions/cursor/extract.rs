use super::db::open_cursor_db;
use super::db::read_workspace_composer_data;
use super::model::CursorThreadHead;
use super::model::CursorThreadHeadSet;
use super::model::parse_workspace_cwd;
use super::model::parse_workspace_thread_heads;
use std::io;
use std::path::Path;
use std::path::PathBuf;
use tracing::warn;

pub(super) async fn load_workspace_heads(
    workspace_storage: &Path,
    fallback_cwd: &Path,
) -> io::Result<Vec<CursorThreadHead>> {
    let mut head_set = CursorThreadHeadSet::default();
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
        let composer_data = read_workspace_composer_data(&pool).await?;
        pool.close().await;
        let Some(composer_data) = composer_data else {
            continue;
        };
        let heads = match parse_workspace_thread_heads(&composer_data, &cwd) {
            Ok(heads) => heads,
            Err(err) => {
                warn!(
                    "failed to parse Cursor workspace composer data {}: {err}",
                    state_db.display()
                );
                continue;
            }
        };
        head_set.extend(heads);
    }

    Ok(head_set.into_sorted_vec())
}

fn read_workspace_cwd(workspace_dir: &Path) -> Option<PathBuf> {
    let workspace_json = std::fs::read_to_string(workspace_dir.join("workspace.json")).ok()?;
    parse_workspace_cwd(&workspace_json)
}
