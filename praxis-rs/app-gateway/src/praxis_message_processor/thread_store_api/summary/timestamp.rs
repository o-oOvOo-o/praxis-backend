use chrono::DateTime;
use chrono::SecondsFormat;
use chrono::Utc;
use std::path::Path;

pub(super) fn non_empty_timestamp(timestamp: &str) -> Option<String> {
    non_empty_timestamp_str(timestamp).map(str::to_string)
}

pub(super) fn non_empty_timestamp_str(timestamp: &str) -> Option<&str> {
    if timestamp.is_empty() {
        None
    } else {
        Some(timestamp)
    }
}

pub(super) async fn read_updated_at(path: &Path, created_at: Option<&str>) -> Option<String> {
    let updated_at = tokio::fs::metadata(path)
        .await
        .ok()
        .and_then(|meta| meta.modified().ok())
        .map(|modified| {
            let updated_at: DateTime<Utc> = modified.into();
            updated_at.to_rfc3339_opts(SecondsFormat::Secs, true)
        });
    updated_at.or_else(|| created_at.map(str::to_string))
}
