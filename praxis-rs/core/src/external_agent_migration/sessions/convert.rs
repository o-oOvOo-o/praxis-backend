use super::record::ExternalSessionRecord;
use super::source::ExternalAgentSource;
use chrono::DateTime;
use chrono::SecondsFormat;
use chrono::Utc;
use praxis_protocol::ThreadId;
use praxis_protocol::protocol::AgentMessageEvent;
use praxis_protocol::protocol::EventMsg;
use praxis_protocol::protocol::RolloutItem;
use praxis_protocol::protocol::SessionMeta;
use praxis_protocol::protocol::SessionMetaLine;
use praxis_protocol::protocol::SessionSource;
use praxis_protocol::protocol::UserMessageEvent;
use std::path::PathBuf;
use uuid::Uuid;

pub(super) struct ExternalSessionBuilder {
    thread_id: ThreadId,
    title: Option<String>,
    created_at: DateTime<Utc>,
    items: Vec<(String, RolloutItem)>,
}

impl ExternalSessionBuilder {
    pub(super) fn new(
        source: ExternalAgentSource,
        external_id: &str,
        title: Option<String>,
        cwd: Option<PathBuf>,
        created_at: DateTime<Utc>,
    ) -> Option<Self> {
        let thread_id = thread_id_from_source(source, external_id)?;
        Some(Self {
            thread_id,
            title,
            created_at,
            items: vec![session_meta_line(source, thread_id, cwd, created_at)],
        })
    }

    pub(super) fn push_user_message(&mut self, timestamp: DateTime<Utc>, text: &str) {
        self.items.push(user_message_line(timestamp, text));
    }

    pub(super) fn push_agent_message(&mut self, timestamp: DateTime<Utc>, text: &str) {
        self.items.push(agent_message_line(timestamp, text));
    }

    pub(super) fn finish(self) -> Option<ExternalSessionRecord> {
        (self.items.len() > 1).then(|| ExternalSessionRecord {
            thread_id: self.thread_id,
            title: self.title,
            created_at: self.created_at,
            items: self.items,
        })
    }
}

fn thread_id_from_source(source: ExternalAgentSource, external_id: &str) -> Option<ThreadId> {
    let uuid = Uuid::new_v5(
        &Uuid::NAMESPACE_URL,
        format!("praxis:{}:{external_id}", source.import_model_provider_id()).as_bytes(),
    );
    ThreadId::from_string(&uuid.to_string()).ok()
}

fn timestamp_string(timestamp: DateTime<Utc>) -> String {
    timestamp.to_rfc3339_opts(SecondsFormat::Millis, true)
}

fn session_meta_line(
    source: ExternalAgentSource,
    thread_id: ThreadId,
    cwd: Option<PathBuf>,
    created_at: DateTime<Utc>,
) -> (String, RolloutItem) {
    let mut meta = SessionMeta::default();
    meta.id = thread_id;
    meta.timestamp = timestamp_string(created_at);
    if let Some(cwd) = cwd {
        meta.cwd = cwd;
    }
    source.apply_session_meta_identity(&mut meta);
    meta.cli_version = env!("CARGO_PKG_VERSION").to_string();
    meta.source = SessionSource::VSCode;
    meta.memory_mode = Some("disabled".to_string());
    (
        timestamp_string(created_at),
        RolloutItem::SessionMeta(SessionMetaLine { meta, git: None }),
    )
}

fn user_message_line(timestamp: DateTime<Utc>, text: &str) -> (String, RolloutItem) {
    (
        timestamp_string(timestamp),
        RolloutItem::EventMsg(EventMsg::UserMessage(UserMessageEvent {
            message: text.to_string(),
            images: Some(Vec::new()),
            local_images: Vec::new(),
            text_elements: Vec::new(),
        })),
    )
}

fn agent_message_line(timestamp: DateTime<Utc>, text: &str) -> (String, RolloutItem) {
    (
        timestamp_string(timestamp),
        RolloutItem::EventMsg(EventMsg::AgentMessage(AgentMessageEvent {
            message: text.to_string(),
            phase: None,
            memory_citation: None,
        })),
    )
}
