use super::record::ExternalSessionRecord;
use super::source::ExternalAgentSource;
use crate::config::Config;
use praxis_protocol::protocol::RolloutItem;
use praxis_protocol::protocol::RolloutLine;
use std::io;
use std::path::Path;
use std::path::PathBuf;
use tokio::fs;
use tracing::warn;

pub(super) struct ExternalSessionStore<'a> {
    config: &'a Config,
    source: ExternalAgentSource,
    state_db: Option<praxis_rollout::state_db::StateDbHandle>,
}

impl<'a> ExternalSessionStore<'a> {
    pub(super) async fn open(config: &'a Config, source: ExternalAgentSource) -> Self {
        let state_db = praxis_rollout::state_db::try_get_state_db(config)
            .await
            .ok();
        Self {
            config,
            source,
            state_db,
        }
    }

    pub(super) async fn persist(&self, record: &ExternalSessionRecord) -> io::Result<()> {
        let path = self.rollout_path(record);
        write_rollout(&path, &record.items).await?;
        let rollout_items = record
            .items
            .iter()
            .map(|(_, item)| item.clone())
            .collect::<Vec<_>>();
        praxis_rollout::state_db::reconcile_rollout(
            self.state_db.as_deref(),
            &path,
            self.source.import_model_provider_id(),
            None,
            &rollout_items,
            Some(false),
            Some("disabled"),
        )
        .await;
        self.persist_title(record).await;
        Ok(())
    }

    fn rollout_path(&self, record: &ExternalSessionRecord) -> PathBuf {
        let year = record.created_at.format("%Y").to_string();
        let month = record.created_at.format("%m").to_string();
        let day = record.created_at.format("%d").to_string();
        let file_ts = record.created_at.format("%Y-%m-%dT%H-%M-%S").to_string();
        self.config
            .praxis_home
            .join(praxis_rollout::SESSIONS_SUBDIR)
            .join(year)
            .join(month)
            .join(day)
            .join(format!("rollout-{file_ts}-{}.jsonl", record.thread_id))
    }

    async fn persist_title(&self, record: &ExternalSessionRecord) {
        let Some(state_db) = self.state_db.as_deref() else {
            return;
        };
        let Some(title) = record
            .title
            .as_deref()
            .and_then(crate::util::normalize_thread_name)
        else {
            return;
        };
        if let Err(err) = praxis_rollout::ThreadNameWriter::new(Some(state_db))
            .write_name(record.thread_id, &title)
            .await
        {
            warn!(
                "failed to persist {} external thread name for {}: {err}",
                self.source.import_model_provider_id(),
                record.thread_id
            );
        }
    }
}

async fn write_rollout(path: &Path, items: &[(String, RolloutItem)]) -> io::Result<()> {
    let parent = path.parent().ok_or_else(|| {
        io::Error::other(format!("rollout path has no parent: {}", path.display()))
    })?;
    fs::create_dir_all(parent).await?;
    let mut jsonl = String::new();
    for (timestamp, item) in items {
        let line = RolloutLine {
            timestamp: timestamp.clone(),
            item: item.clone(),
        };
        let serialized = serde_json::to_string(&line)
            .map_err(|err| io::Error::other(format!("serialize external rollout: {err}")))?;
        jsonl.push_str(&serialized);
        jsonl.push('\n');
    }
    fs::write(path, jsonl).await
}
