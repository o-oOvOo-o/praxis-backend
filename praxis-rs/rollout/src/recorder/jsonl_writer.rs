use std::io::Error as IoError;

use praxis_protocol::protocol::RolloutItem;
use time::OffsetDateTime;
use time::format_description::FormatItem;
use time::macros::format_description;
use tokio::io::AsyncWriteExt;
use tokio::io::BufWriter;

pub(super) struct JsonlWriter {
    file: BufWriter<tokio::fs::File>,
    needs_flush: bool,
}

#[derive(serde::Serialize)]
struct RolloutLineRef<'a> {
    timestamp: String,
    #[serde(flatten)]
    item: &'a RolloutItem,
}

impl JsonlWriter {
    pub(super) fn new(file: tokio::fs::File) -> Self {
        Self {
            file: BufWriter::new(file),
            needs_flush: false,
        }
    }

    pub(super) async fn write_rollout_item(
        &mut self,
        rollout_item: &RolloutItem,
    ) -> std::io::Result<()> {
        let timestamp_format: &[FormatItem] = format_description!(
            "[year]-[month]-[day]T[hour]:[minute]:[second].[subsecond digits:3]Z"
        );
        let timestamp = OffsetDateTime::now_utc()
            .format(timestamp_format)
            .map_err(|e| IoError::other(format!("failed to format timestamp: {e}")))?;

        let line = RolloutLineRef {
            timestamp,
            item: rollout_item,
        };
        self.write_line(&line).await
    }
    async fn write_line(&mut self, item: &impl serde::Serialize) -> std::io::Result<()> {
        let mut json = serde_json::to_string(item)?;
        json.push('\n');
        self.file.write_all(json.as_bytes()).await?;
        self.needs_flush = true;
        Ok(())
    }

    pub(super) async fn flush(&mut self) -> std::io::Result<()> {
        if self.needs_flush {
            self.file.flush().await?;
            self.needs_flush = false;
        }
        Ok(())
    }
}
