use sqlx::QueryBuilder;
use sqlx::Row;
use sqlx::Sqlite;
use sqlx::SqlitePool;
use sqlx::sqlite::SqliteConnectOptions;
use sqlx::sqlite::SqlitePoolOptions;
use std::collections::HashMap;
use std::io;
use std::path::Path;
use std::time::Duration;

const CURSOR_DISK_KV_TABLE: &str = "cursorDiskKV";
const SQLITE_BIND_CHUNK: usize = 250;
const WORKSPACE_COMPOSER_DATA_KEY: &str = "composer.composerData";
const WORKSPACE_ITEM_TABLE: &str = "ItemTable";

pub(super) async fn open_cursor_db(path: &Path) -> io::Result<SqlitePool> {
    let options = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(false)
        .read_only(true)
        .busy_timeout(Duration::from_secs(2));
    SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .map_err(io_error)
}

pub(super) async fn read_cursor_disk_values(
    pool: &SqlitePool,
    keys: &[String],
) -> io::Result<HashMap<String, String>> {
    read_kv_values(pool, CURSOR_DISK_KV_TABLE, keys).await
}

pub(super) async fn read_workspace_composer_data(pool: &SqlitePool) -> io::Result<Option<String>> {
    read_kv_value(pool, WORKSPACE_ITEM_TABLE, WORKSPACE_COMPOSER_DATA_KEY).await
}

async fn read_kv_value(
    pool: &SqlitePool,
    table: &'static str,
    key: &str,
) -> io::Result<Option<String>> {
    let keys = vec![key.to_string()];
    let mut values = read_kv_values(pool, table, &keys).await?;
    Ok(values.remove(key))
}

async fn read_kv_values(
    pool: &SqlitePool,
    table: &'static str,
    keys: &[String],
) -> io::Result<HashMap<String, String>> {
    let mut values = HashMap::new();
    for chunk in keys.chunks(SQLITE_BIND_CHUNK) {
        if chunk.is_empty() {
            continue;
        }
        let mut query = QueryBuilder::<Sqlite>::new("SELECT key, value FROM ");
        query.push(table);
        query.push(" WHERE key IN (");
        for (index, key) in chunk.iter().enumerate() {
            if index > 0 {
                query.push(", ");
            }
            query.push_bind(key);
        }
        query.push(")");
        let rows = query.build().fetch_all(pool).await.map_err(io_error)?;
        for row in rows {
            let key: String = row.try_get("key").map_err(io_error)?;
            if let Some(value) = row_value_to_string(&row)? {
                values.insert(key, value);
            }
        }
    }
    Ok(values)
}

fn row_value_to_string(row: &sqlx::sqlite::SqliteRow) -> io::Result<Option<String>> {
    if let Ok(value) = row.try_get::<String, _>("value") {
        return Ok(Some(value));
    }
    if let Ok(bytes) = row.try_get::<Vec<u8>, _>("value") {
        return match String::from_utf8(bytes) {
            Ok(value) => Ok(Some(value)),
            Err(err) => Err(io::Error::new(io::ErrorKind::InvalidData, err)),
        };
    }
    Ok(None)
}

fn io_error(error: impl std::error::Error + Send + Sync + 'static) -> io::Error {
    io::Error::other(error)
}
