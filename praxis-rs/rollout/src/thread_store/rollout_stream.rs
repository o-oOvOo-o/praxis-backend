use std::io;
use std::io::Error as IoError;
use std::path::Path;

use praxis_protocol::ThreadId;
use praxis_protocol::protocol::RolloutItem;
use praxis_protocol::protocol::RolloutLine;
use tokio::io::AsyncBufReadExt;
use tokio::io::BufReader;
use tracing::trace;
use tracing::warn;

pub(crate) struct RolloutItemStream {
    lines: tokio::io::Lines<BufReader<tokio::fs::File>>,
    thread_id: Option<ThreadId>,
    parse_errors: usize,
    item_count: usize,
    saw_non_empty_line: bool,
}

impl RolloutItemStream {
    pub(crate) async fn open(path: &Path) -> io::Result<Self> {
        trace!("Reading persisted thread from {path:?}");
        let file = tokio::fs::File::open(path).await?;
        Ok(Self {
            lines: BufReader::new(file).lines(),
            thread_id: None,
            parse_errors: 0,
            item_count: 0,
            saw_non_empty_line: false,
        })
    }

    pub(crate) async fn next_item(&mut self) -> io::Result<Option<RolloutItem>> {
        while let Some(line) = self.lines.next_line().await? {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            self.saw_non_empty_line = true;
            match serde_json::from_str::<RolloutLine>(line) {
                Ok(rollout_line) => {
                    let item = rollout_line.item;
                    if self.thread_id.is_none()
                        && let RolloutItem::SessionMeta(session_meta) = &item
                    {
                        self.thread_id = Some(session_meta.meta.id);
                    }
                    self.item_count = self.item_count.saturating_add(1);
                    return Ok(Some(item));
                }
                Err(error) => {
                    warn!("failed to parse persisted thread line: {line:?}, error: {error}");
                    self.parse_errors = self.parse_errors.saturating_add(1);
                }
            }
        }
        if !self.saw_non_empty_line {
            return Err(IoError::other("empty session file"));
        }
        Ok(None)
    }

    pub(crate) fn outcome(&self) -> (Option<ThreadId>, usize) {
        (self.thread_id, self.parse_errors)
    }

    pub(crate) fn item_count(&self) -> usize {
        self.item_count
    }
}
