use std::io;
use std::path::{Path, PathBuf};

pub(super) fn path(
    home: &Path,
    summary: &praxis_thread_store::ThreadSummary,
) -> io::Result<PathBuf> {
    let created =
        chrono::DateTime::from_timestamp_millis(summary.created_at_unix_ms).ok_or_else(|| {
            io::Error::other("native thread creation time is outside the locator range")
        })?;
    let directory = if summary.archived {
        home.join(crate::ARCHIVED_SESSIONS_SUBDIR)
    } else {
        home.join(crate::SESSIONS_SUBDIR)
            .join(created.format("%Y/%m/%d").to_string())
    };
    Ok(directory.join(format!(
        "rollout-{}-{}.jsonl",
        created.format("%Y-%m-%dT%H-%M-%S"),
        summary.thread_id
    )))
}
