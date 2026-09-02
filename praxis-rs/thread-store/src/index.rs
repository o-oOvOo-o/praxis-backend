use crate::ThreadListPage;
use crate::ThreadListQuery;
use crate::ThreadListSort;
use crate::ThreadStoreError;
use crate::ThreadSummary;
use crate::projection::NativeTranscriptIndex;
use crate::projection::ThreadIndexProjection;
use crate::projection::TranscriptScanPlan;
use crate::projection::TurnCheckpoint;
use praxis_thread_store_contracts::Digest;
use praxis_thread_store_contracts::ThreadId;
use praxis_thread_store_contracts::ThreadRevision;
use sqlx::QueryBuilder;
use sqlx::Row;
use sqlx::Sqlite;
use sqlx::SqlitePool;
use sqlx::sqlite::SqliteConnectOptions;
use sqlx::sqlite::SqlitePoolOptions;
use std::path::Path;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::OnceLock;
use std::sync::Weak;
use tokio::sync::OnceCell;

mod write;

const CURRENT_PROJECTION_GENERATION: i64 = 5;
const TURN_CHECKPOINT_INSERT_BATCH: usize = 128;

#[derive(Clone, Debug)]
pub(crate) struct ThreadIndex {
    shared: Arc<SharedIndex>,
}

#[derive(Debug)]
struct SharedIndex {
    pool: SqlitePool,
    initialized: OnceCell<()>,
}

impl ThreadIndex {
    pub(crate) fn new(root: &Path) -> Self {
        let database_path = root.join("index.sqlite3");
        let options = SqliteConnectOptions::new()
            .filename(&database_path)
            .create_if_missing(true)
            .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
            .foreign_keys(true);
        Self {
            shared: shared_index(database_path, options),
        }
    }

    pub(crate) async fn initialize(&self) -> Result<(), ThreadStoreError> {
        self.shared
            .initialized
            .get_or_try_init(|| async {
                let projection_schema = format!(
                    "CREATE TABLE IF NOT EXISTS thread_projection (
                thread_id TEXT PRIMARY KEY NOT NULL,
                revision INTEGER NOT NULL,
                source TEXT NOT NULL,
                workspace TEXT NOT NULL,
                name TEXT,
                summary TEXT,
                archived INTEGER NOT NULL,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL,
                preview TEXT,
                first_user_message TEXT,
                total_cost_micros INTEGER,
                last_cost_micros INTEGER,
                model TEXT,
                model_provider TEXT,
                reasoning_effort TEXT,
                last_agent_sequence INTEGER NOT NULL DEFAULT 0,
                model_context_checkpoint_revision INTEGER,
                dynamic_tools_digest BLOB,
                transcript_total_turns INTEGER NOT NULL DEFAULT 0,
                projection_generation INTEGER NOT NULL DEFAULT {}
            )",
                    CURRENT_PROJECTION_GENERATION
                );
                sqlx::query(&projection_schema)
                    .execute(&self.shared.pool)
                    .await?;
                migrate_projection_schema(&self.shared.pool).await?;
                sqlx::query(
                    "CREATE TABLE IF NOT EXISTS thread_turn_checkpoint (
                        thread_id TEXT NOT NULL,
                        ordinal INTEGER NOT NULL,
                        scan_after_revision INTEGER NOT NULL,
                        previous_agent_sequence INTEGER,
                        PRIMARY KEY (thread_id, ordinal),
                        FOREIGN KEY (thread_id) REFERENCES thread_projection(thread_id)
                            ON DELETE CASCADE
                    )",
                )
                .execute(&self.shared.pool)
                .await?;
                sqlx::query(
                    "CREATE INDEX IF NOT EXISTS thread_projection_list
             ON thread_projection(archived, updated_at DESC, thread_id DESC)",
                )
                .execute(&self.shared.pool)
                .await?;
                sqlx::query(
                    "CREATE INDEX IF NOT EXISTS thread_projection_created_list
             ON thread_projection(archived, created_at DESC, thread_id DESC)",
                )
                .execute(&self.shared.pool)
                .await?;
                Ok::<(), ThreadStoreError>(())
            })
            .await?;
        Ok(())
    }

    pub(crate) async fn synchronize(
        &self,
        projection: &ThreadIndexProjection,
    ) -> Result<(), ThreadStoreError> {
        self.write_projection(projection, false).await
    }

    pub(crate) async fn replace(
        &self,
        projection: &ThreadIndexProjection,
    ) -> Result<(), ThreadStoreError> {
        self.write_projection(projection, true).await
    }

    async fn write_projection(
        &self,
        projection: &ThreadIndexProjection,
        force: bool,
    ) -> Result<(), ThreadStoreError> {
        let summary = &projection.summary;
        let thread_id = summary.thread_id;
        let thread_key = thread_id.to_string();
        let revision = revision_to_i64(summary.revision)?;
        let checkpoint_rows = projection
            .transcript_index
            .checkpoints
            .iter()
            .map(|checkpoint| {
                Ok::<_, ThreadStoreError>((
                    i64::try_from(checkpoint.ordinal)
                        .map_err(|_| ThreadStoreError::RevisionOverflow)?,
                    revision_to_i64(checkpoint.scan_after)?,
                    checkpoint
                        .previous_agent_sequence
                        .map(i64::try_from)
                        .transpose()
                        .map_err(|_| ThreadStoreError::RevisionOverflow)?,
                ))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let mut transaction = self.shared.pool.begin().await?;
        sqlx::query(
            "INSERT INTO thread_projection
                (thread_id, revision, source, workspace, name, summary, archived,
                 created_at, updated_at, preview, first_user_message, total_cost_micros,
                 last_cost_micros, model, model_provider, reasoning_effort, last_agent_sequence,
                 model_context_checkpoint_revision, dynamic_tools_digest,
                 transcript_total_turns,
                 projection_generation)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(thread_id) DO UPDATE SET
                revision=excluded.revision, source=excluded.source,
                workspace=excluded.workspace, name=excluded.name,
                summary=excluded.summary, archived=excluded.archived,
                created_at=excluded.created_at, updated_at=excluded.updated_at,
                preview=excluded.preview,
                first_user_message=excluded.first_user_message,
                total_cost_micros=excluded.total_cost_micros,
                last_cost_micros=excluded.last_cost_micros,
                model=excluded.model,
                model_provider=excluded.model_provider,
                reasoning_effort=excluded.reasoning_effort,
                last_agent_sequence=excluded.last_agent_sequence,
                model_context_checkpoint_revision=excluded.model_context_checkpoint_revision,
                dynamic_tools_digest=excluded.dynamic_tools_digest,
                transcript_total_turns=excluded.transcript_total_turns,
                projection_generation=excluded.projection_generation
             WHERE ?
                OR thread_projection.revision <> excluded.revision
                OR thread_projection.projection_generation <> excluded.projection_generation",
        )
        .bind(&thread_key)
        .bind(revision)
        .bind(&summary.source)
        .bind(&summary.workspace)
        .bind(&summary.name)
        .bind(&summary.summary)
        .bind(summary.archived)
        .bind(summary.created_at_unix_ms)
        .bind(summary.updated_at_unix_ms)
        .bind(&summary.preview)
        .bind(&summary.first_user_message)
        .bind(summary.total_cost_micros)
        .bind(summary.last_cost_micros)
        .bind(&summary.model)
        .bind(&summary.model_provider)
        .bind(&summary.reasoning_effort)
        .bind(
            i64::try_from(projection.last_agent_sequence)
                .map_err(|_| ThreadStoreError::RevisionOverflow)?,
        )
        .bind(
            projection
                .model_context_checkpoint
                .map(revision_to_i64)
                .transpose()?,
        )
        .bind(
            projection
                .dynamic_tools_digest
                .map(|digest| digest.as_bytes().to_vec()),
        )
        .bind(
            i64::try_from(projection.transcript_index.total_turns)
                .map_err(|_| ThreadStoreError::RevisionOverflow)?,
        )
        .bind(CURRENT_PROJECTION_GENERATION)
        .bind(force)
        .execute(&mut *transaction)
        .await?;
        sqlx::query("DELETE FROM thread_turn_checkpoint WHERE thread_id = ?")
            .bind(&thread_key)
            .execute(&mut *transaction)
            .await?;
        for checkpoint_chunk in checkpoint_rows.chunks(TURN_CHECKPOINT_INSERT_BATCH) {
            let mut query = QueryBuilder::<Sqlite>::new(
                "INSERT INTO thread_turn_checkpoint
                    (thread_id, ordinal, scan_after_revision, previous_agent_sequence) ",
            );
            query.push_values(
                checkpoint_chunk,
                |mut row, (ordinal, scan_after, previous_sequence)| {
                    row.push_bind(&thread_key)
                        .push_bind(*ordinal)
                        .push_bind(*scan_after)
                        .push_bind(*previous_sequence);
                },
            );
            query.build().execute(&mut *transaction).await?;
        }
        transaction.commit().await?;
        Ok(())
    }

    pub(crate) async fn list(
        &self,
        query: ThreadListQuery,
    ) -> Result<ThreadListPage, ThreadStoreError> {
        let limit = query.limit.unwrap_or(50).clamp(1, 200);
        let cursor = ThreadListCursor::decode(query.cursor.as_deref(), query.sort);
        let search = query.search.map(|value| format!("%{}%", value));
        let sort_column = query.sort.column();
        let mut sql = QueryBuilder::<Sqlite>::new(
            "SELECT thread_id, revision, source, workspace, name, summary, archived,
                    created_at, updated_at, preview, first_user_message,
                    total_cost_micros, last_cost_micros, model, model_provider, reasoning_effort
             FROM thread_projection WHERE 1 = 1",
        );
        if let Some(archived) = query.archived {
            sql.push(" AND archived = ").push_bind(archived);
        }
        if let Some(workspace) = query.workspace {
            sql.push(" AND workspace = ").push_bind(workspace);
        }
        if let Some(source) = query.source {
            sql.push(" AND source = ").push_bind(source);
        }
        if let Some(search) = search.as_deref() {
            sql.push(" AND (name LIKE ")
                .push_bind(search)
                .push(" OR summary LIKE ")
                .push_bind(search)
                .push(" OR preview LIKE ")
                .push_bind(search)
                .push(')');
        }
        if let Some(cursor) = cursor {
            sql.push(" AND (")
                .push(sort_column)
                .push(", thread_id) < (")
                .push_bind(cursor.sort_value)
                .push(", ")
                .push_bind(cursor.thread_id.to_string())
                .push(')');
        }
        sql.push(" ORDER BY ")
            .push(sort_column)
            .push(" DESC, thread_id DESC LIMIT ")
            .push_bind(i64::try_from(limit + 1).unwrap_or(i64::MAX));
        let rows = sql.build().fetch_all(&self.shared.pool).await?;
        let has_more = rows.len() > limit;
        let items = rows
            .into_iter()
            .take(limit)
            .map(row_to_summary)
            .collect::<Result<Vec<_>, _>>()?;
        let next_cursor = if has_more {
            items
                .last()
                .map(|item| ThreadListCursor::encode(item, query.sort))
        } else {
            None
        };
        Ok(ThreadListPage { next_cursor, items })
    }

    pub(crate) async fn read_summary(
        &self,
        thread_id: ThreadId,
    ) -> Result<Option<ThreadSummary>, ThreadStoreError> {
        self.initialize().await?;
        sqlx::query(
            "SELECT thread_id, revision, source, workspace, name, summary, archived,
                    created_at, updated_at, preview, first_user_message,
                    total_cost_micros, last_cost_micros, model, model_provider, reasoning_effort
             FROM thread_projection
             WHERE thread_id = ? AND projection_generation = ?",
        )
        .bind(thread_id.to_string())
        .bind(CURRENT_PROJECTION_GENERATION)
        .fetch_optional(&self.shared.pool)
        .await?
        .map(row_to_summary)
        .transpose()
    }

    pub(crate) async fn read_projection(
        &self,
        thread_id: ThreadId,
    ) -> Result<Option<ThreadIndexProjection>, ThreadStoreError> {
        self.initialize().await?;
        let mut transaction = self.shared.pool.begin().await?;
        let row = sqlx::query(
            "SELECT thread_id, revision, source, workspace, name, summary, archived,
                    created_at, updated_at, preview, first_user_message,
                    total_cost_micros, last_cost_micros, model, model_provider, reasoning_effort,
                    last_agent_sequence, model_context_checkpoint_revision,
                    dynamic_tools_digest, transcript_total_turns
             FROM thread_projection
             WHERE thread_id = ? AND projection_generation = ?",
        )
        .bind(thread_id.to_string())
        .bind(CURRENT_PROJECTION_GENERATION)
        .fetch_optional(&mut *transaction)
        .await?;
        let Some(row) = row else {
            transaction.commit().await?;
            return Ok(None);
        };
        let mut projection = row_to_projection(row)?;
        let checkpoints = sqlx::query(
            "SELECT ordinal, scan_after_revision, previous_agent_sequence
             FROM thread_turn_checkpoint WHERE thread_id = ? ORDER BY ordinal",
        )
        .bind(thread_id.to_string())
        .fetch_all(&mut *transaction)
        .await?;
        projection.transcript_index.checkpoints = checkpoints
            .into_iter()
            .map(row_to_turn_checkpoint)
            .collect::<Result<_, _>>()?;
        transaction.commit().await?;
        Ok(Some(projection))
    }

    pub(crate) async fn transcript_scan_plan<F>(
        &self,
        thread_id: ThreadId,
        select_first_turn: F,
    ) -> Result<Option<TranscriptScanPlan>, ThreadStoreError>
    where
        F: FnOnce(usize) -> Option<usize>,
    {
        self.initialize().await?;
        let thread_key = thread_id.to_string();
        let mut transaction = self.shared.pool.begin().await?;
        let frontier = sqlx::query_as::<_, (i64, i64, i64)>(
            "SELECT revision, transcript_total_turns, last_agent_sequence
             FROM thread_projection
             WHERE thread_id = ? AND projection_generation = ?",
        )
        .bind(&thread_key)
        .bind(CURRENT_PROJECTION_GENERATION)
        .fetch_optional(&mut *transaction)
        .await?;
        let Some((revision, total_turns, last_agent_sequence)) = frontier else {
            transaction.commit().await?;
            return Ok(None);
        };
        let index = NativeTranscriptIndex {
            total_turns: u64::try_from(total_turns)
                .map_err(|_| ThreadStoreError::RevisionOverflow)?,
            through_revision: revision_from_i64(revision)?,
            frontier_sequence: (last_agent_sequence != 0)
                .then(|| u64::try_from(last_agent_sequence))
                .transpose()
                .map_err(|_| ThreadStoreError::RevisionOverflow)?,
            ..NativeTranscriptIndex::default()
        };
        let Some(first_turn) = select_first_turn(index.total_turns()) else {
            transaction.commit().await?;
            return Ok(None);
        };
        let first_turn_u64 = u64::try_from(first_turn)
            .unwrap_or(u64::MAX)
            .min(index.total_turns);
        let checkpoint = if first_turn_u64 == 0 || first_turn_u64 == index.total_turns {
            None
        } else {
            let ordinal =
                i64::try_from(first_turn_u64).map_err(|_| ThreadStoreError::RevisionOverflow)?;
            sqlx::query(
                "SELECT ordinal, scan_after_revision, previous_agent_sequence
                 FROM thread_turn_checkpoint WHERE thread_id = ? AND ordinal = ?",
            )
            .bind(&thread_key)
            .bind(ordinal)
            .fetch_optional(&mut *transaction)
            .await?
            .map(row_to_turn_checkpoint)
            .transpose()?
        };
        let plan = index.plan_from_checkpoint(first_turn, checkpoint);
        transaction.commit().await?;
        Ok(plan)
    }

    pub(crate) async fn latest_agent_event_sequence(
        &self,
        thread_id: ThreadId,
    ) -> Result<u64, ThreadStoreError> {
        self.initialize().await?;
        let sequence = sqlx::query_scalar::<_, i64>(
            "SELECT last_agent_sequence FROM thread_projection
             WHERE thread_id = ? AND projection_generation = ?",
        )
        .bind(thread_id.to_string())
        .bind(CURRENT_PROJECTION_GENERATION)
        .fetch_optional(&self.shared.pool)
        .await?;
        sequence
            .map(|value| u64::try_from(value).map_err(|_| ThreadStoreError::RevisionOverflow))
            .transpose()
            .map(|value| value.unwrap_or_default())
    }

    pub(crate) async fn model_context_checkpoint(
        &self,
        thread_id: ThreadId,
    ) -> Result<Option<ThreadRevision>, ThreadStoreError> {
        self.initialize().await?;
        sqlx::query_scalar::<_, Option<i64>>(
            "SELECT model_context_checkpoint_revision FROM thread_projection
             WHERE thread_id = ? AND projection_generation = ?",
        )
        .bind(thread_id.to_string())
        .bind(CURRENT_PROJECTION_GENERATION)
        .fetch_optional(&self.shared.pool)
        .await?
        .flatten()
        .map(revision_from_i64)
        .transpose()
    }

    pub(crate) async fn remove(&self, thread_id: ThreadId) -> Result<(), ThreadStoreError> {
        self.initialize().await?;
        sqlx::query("DELETE FROM thread_projection WHERE thread_id = ?")
            .bind(thread_id.to_string())
            .execute(&self.shared.pool)
            .await?;
        Ok(())
    }
}

fn shared_index(path: std::path::PathBuf, options: SqliteConnectOptions) -> Arc<SharedIndex> {
    static INDEXES: OnceLock<
        Mutex<std::collections::HashMap<std::path::PathBuf, Weak<SharedIndex>>>,
    > = OnceLock::new();
    let mut indexes = INDEXES
        .get_or_init(|| Mutex::new(std::collections::HashMap::new()))
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    indexes.retain(|_, index| index.strong_count() > 0);
    if let Some(index) = indexes.get(&path).and_then(Weak::upgrade) {
        return index;
    }
    let index = Arc::new(SharedIndex {
        pool: SqlitePoolOptions::new()
            .max_connections(4)
            .connect_lazy_with(options),
        initialized: OnceCell::new(),
    });
    indexes.insert(path, Arc::downgrade(&index));
    index
}

async fn migrate_projection_schema(pool: &SqlitePool) -> Result<(), ThreadStoreError> {
    let columns = sqlx::query("PRAGMA table_info(thread_projection)")
        .fetch_all(pool)
        .await?;
    if !columns
        .iter()
        .any(|row| row.try_get::<String, _>("name").ok().as_deref() == Some("last_agent_sequence"))
    {
        sqlx::query(
            "ALTER TABLE thread_projection
             ADD COLUMN last_agent_sequence INTEGER NOT NULL DEFAULT 0",
        )
        .execute(pool)
        .await?;
    }
    if !columns.iter().any(|row| {
        row.try_get::<String, _>("name").ok().as_deref() == Some("projection_generation")
    }) {
        sqlx::query(
            "ALTER TABLE thread_projection
             ADD COLUMN projection_generation INTEGER NOT NULL DEFAULT 0",
        )
        .execute(pool)
        .await?;
    }
    if !columns
        .iter()
        .any(|row| row.try_get::<String, _>("name").ok().as_deref() == Some("dynamic_tools_digest"))
    {
        sqlx::query("ALTER TABLE thread_projection ADD COLUMN dynamic_tools_digest BLOB")
            .execute(pool)
            .await?;
    }
    if !columns.iter().any(|row| {
        row.try_get::<String, _>("name").ok().as_deref() == Some("transcript_total_turns")
    }) {
        sqlx::query(
            "ALTER TABLE thread_projection
             ADD COLUMN transcript_total_turns INTEGER NOT NULL DEFAULT 0",
        )
        .execute(pool)
        .await?;
    }
    if !columns
        .iter()
        .any(|row| row.try_get::<String, _>("name").ok().as_deref() == Some("first_user_message"))
    {
        sqlx::query("ALTER TABLE thread_projection ADD COLUMN first_user_message TEXT")
            .execute(pool)
            .await?;
    }
    for column in ["total_cost_micros", "last_cost_micros"] {
        if !columns
            .iter()
            .any(|row| row.try_get::<String, _>("name").ok().as_deref() == Some(column))
        {
            sqlx::query(&format!(
                "ALTER TABLE thread_projection ADD COLUMN {column} INTEGER"
            ))
            .execute(pool)
            .await?;
        }
    }
    for column in ["model", "model_provider", "reasoning_effort"] {
        if !columns
            .iter()
            .any(|row| row.try_get::<String, _>("name").ok().as_deref() == Some(column))
        {
            sqlx::query(&format!(
                "ALTER TABLE thread_projection ADD COLUMN {column} TEXT"
            ))
            .execute(pool)
            .await?;
        }
    }
    if !columns.iter().any(|row| {
        row.try_get::<String, _>("name").ok().as_deref()
            == Some("model_context_checkpoint_revision")
    }) {
        sqlx::query(
            "ALTER TABLE thread_projection
             ADD COLUMN model_context_checkpoint_revision INTEGER",
        )
        .execute(pool)
        .await?;
    }
    // Projection generation changes rebuild derived state from the journal.
    sqlx::query("DROP TABLE IF EXISTS thread_event")
        .execute(pool)
        .await?;
    Ok(())
}

fn row_to_summary(row: sqlx::sqlite::SqliteRow) -> Result<ThreadSummary, ThreadStoreError> {
    let revision: i64 = row.try_get("revision")?;
    Ok(ThreadSummary {
        thread_id: ThreadId::parse(row.try_get("thread_id")?)
            .map_err(|error| ThreadStoreError::Worker(error.to_string()))?,
        revision: revision_from_i64(revision)?,
        source: row.try_get("source")?,
        workspace: row.try_get("workspace")?,
        name: row.try_get("name")?,
        summary: row.try_get("summary")?,
        archived: row.try_get("archived")?,
        created_at_unix_ms: row.try_get("created_at")?,
        updated_at_unix_ms: row.try_get("updated_at")?,
        preview: row.try_get("preview")?,
        first_user_message: row.try_get("first_user_message")?,
        total_cost_micros: row.try_get("total_cost_micros")?,
        last_cost_micros: row.try_get("last_cost_micros")?,
        model: row.try_get("model")?,
        model_provider: row.try_get("model_provider")?,
        reasoning_effort: row.try_get("reasoning_effort")?,
    })
}

fn row_to_projection(
    row: sqlx::sqlite::SqliteRow,
) -> Result<ThreadIndexProjection, ThreadStoreError> {
    let last_agent_sequence = row.try_get::<i64, _>("last_agent_sequence")?;
    let model_context_checkpoint = row
        .try_get::<Option<i64>, _>("model_context_checkpoint_revision")?
        .map(revision_from_i64)
        .transpose()?;
    let transcript_total_turns = row.try_get::<i64, _>("transcript_total_turns")?;
    let dynamic_tools_digest = row
        .try_get::<Option<Vec<u8>>, _>("dynamic_tools_digest")?
        .map(digest_from_blob)
        .transpose()?;
    let summary = row_to_summary(row)?;
    Ok(ThreadIndexProjection {
        transcript_index: crate::projection::NativeTranscriptIndex {
            total_turns: u64::try_from(transcript_total_turns)
                .map_err(|_| ThreadStoreError::RevisionOverflow)?,
            checkpoints: Default::default(),
            through_revision: summary.revision,
            frontier_sequence: (last_agent_sequence != 0)
                .then(|| u64::try_from(last_agent_sequence))
                .transpose()
                .map_err(|_| ThreadStoreError::RevisionOverflow)?,
        },
        summary,
        last_agent_sequence: u64::try_from(last_agent_sequence)
            .map_err(|_| ThreadStoreError::RevisionOverflow)?,
        model_context_checkpoint,
        dynamic_tools_digest,
    })
}

fn digest_from_blob(bytes: Vec<u8>) -> Result<Digest, ThreadStoreError> {
    bytes
        .try_into()
        .map(Digest::from_bytes)
        .map_err(|bytes: Vec<u8>| {
            ThreadStoreError::Worker(format!("invalid projection digest length: {}", bytes.len()))
        })
}

fn row_to_turn_checkpoint(
    row: sqlx::sqlite::SqliteRow,
) -> Result<TurnCheckpoint, ThreadStoreError> {
    let ordinal = row.try_get::<i64, _>("ordinal")?;
    let previous_agent_sequence = row.try_get::<Option<i64>, _>("previous_agent_sequence")?;
    Ok(TurnCheckpoint {
        ordinal: u64::try_from(ordinal).map_err(|_| ThreadStoreError::RevisionOverflow)?,
        scan_after: revision_from_i64(row.try_get("scan_after_revision")?)?,
        previous_agent_sequence: previous_agent_sequence
            .map(u64::try_from)
            .transpose()
            .map_err(|_| ThreadStoreError::RevisionOverflow)?,
    })
}

fn revision_to_i64(revision: ThreadRevision) -> Result<i64, ThreadStoreError> {
    i64::try_from(revision.get()).map_err(|_| ThreadStoreError::RevisionOverflow)
}

fn revision_from_i64(revision: i64) -> Result<ThreadRevision, ThreadStoreError> {
    u64::try_from(revision)
        .map(ThreadRevision::new)
        .map_err(|_| ThreadStoreError::RevisionOverflow)
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ThreadListCursor {
    sort_value: i64,
    thread_id: ThreadId,
}

impl ThreadListCursor {
    fn decode(cursor: Option<&str>, sort: ThreadListSort) -> Option<Self> {
        let mut fields = cursor?.splitn(4, ':');
        if fields.next()? != "v1" || fields.next()? != sort.tag() {
            return None;
        }
        Some(Self {
            sort_value: fields.next()?.parse().ok()?,
            thread_id: ThreadId::parse(fields.next()?).ok()?,
        })
    }

    fn encode(summary: &ThreadSummary, sort: ThreadListSort) -> String {
        let sort_value = match sort {
            ThreadListSort::CreatedAt => summary.created_at_unix_ms,
            ThreadListSort::UpdatedAt => summary.updated_at_unix_ms,
        };
        format!("v1:{}:{sort_value}:{}", sort.tag(), summary.thread_id)
    }
}

impl ThreadListSort {
    const fn column(self) -> &'static str {
        match self {
            Self::CreatedAt => "created_at",
            Self::UpdatedAt => "updated_at",
        }
    }

    const fn tag(self) -> &'static str {
        match self {
            Self::CreatedAt => "c",
            Self::UpdatedAt => "u",
        }
    }
}

#[cfg(test)]
mod cursor_tests {
    use super::*;

    #[test]
    fn cursor_round_trips_the_selected_stable_order() {
        let summary = ThreadSummary {
            thread_id: ThreadId::new(),
            revision: ThreadRevision::new(1),
            source: "run".into(),
            workspace: "F:/Cunning3D".into(),
            name: None,
            summary: None,
            archived: false,
            created_at_unix_ms: 11,
            updated_at_unix_ms: 29,
            preview: None,
            first_user_message: None,
            total_cost_micros: None,
            last_cost_micros: None,
            model: None,
            model_provider: None,
            reasoning_effort: None,
        };

        for sort in [ThreadListSort::CreatedAt, ThreadListSort::UpdatedAt] {
            let encoded = ThreadListCursor::encode(&summary, sort);
            let decoded = ThreadListCursor::decode(Some(&encoded), sort).expect("cursor");
            assert_eq!(decoded.thread_id, summary.thread_id);
            assert_eq!(
                decoded.sort_value,
                match sort {
                    ThreadListSort::CreatedAt => 11,
                    ThreadListSort::UpdatedAt => 29,
                }
            );
        }
    }

    #[test]
    fn cursor_cannot_cross_sort_orders() {
        let thread_id = ThreadId::new();
        let encoded = format!("v1:c:11:{thread_id}");
        assert_eq!(
            ThreadListCursor::decode(Some(&encoded), ThreadListSort::UpdatedAt),
            None
        );
    }

    #[test]
    fn restore_synchronization_is_one_conditional_upsert() {
        let source = include_str!("index.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("index production source");
        let synchronize = source
            .split("pub(crate) async fn synchronize")
            .nth(1)
            .expect("synchronize method")
            .split("pub(crate) async fn replace")
            .next()
            .expect("synchronize body");
        assert!(synchronize.contains("write_projection(projection, false)"));
        assert!(!synchronize.contains("SELECT revision"));
        assert_eq!(source.matches("INSERT INTO thread_projection").count(), 1);
        assert!(source.contains(
            "OR thread_projection.projection_generation <> excluded.projection_generation"
        ));
        assert!(source.contains("projection_generation INTEGER NOT NULL DEFAULT {}"));
        assert!(source.contains("ADD COLUMN projection_generation"));
        assert!(source.contains("INTEGER NOT NULL DEFAULT 0"));
        assert!(source.contains("AND projection_generation = ?"));
        assert!(source.contains("write_projection(projection, true)"));
        assert!(source.contains("const CURRENT_PROJECTION_GENERATION: i64 = 5"));
        assert!(!source.contains("transcript_index_complete"));
        assert!(source.contains("model_context_checkpoint_revision INTEGER"));
        assert!(source.contains("ADD COLUMN model_context_checkpoint_revision"));
        assert!(source.contains("dynamic_tools_digest BLOB"));
        assert!(source.contains("ADD COLUMN dynamic_tools_digest BLOB"));
    }

    #[test]
    fn restore_batches_turn_checkpoint_writes_with_one_thread_key() {
        let source = include_str!("index.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("index production source");
        let write = source
            .split("async fn write_projection")
            .nth(1)
            .expect("projection writer")
            .split("pub(crate) async fn list")
            .next()
            .expect("projection writer body");

        assert!(write.contains("let thread_key = thread_id.to_string();"));
        assert!(write.contains(".chunks(TURN_CHECKPOINT_INSERT_BATCH)"));
        assert!(write.contains("query.push_values"));
        assert!(!write.contains(".bind(thread_id.to_string())"));
        assert!(!write.contains("for checkpoint in &projection.transcript_index.checkpoints"));
        assert!(TURN_CHECKPOINT_INSERT_BATCH * 4 <= 999);
    }

    #[test]
    fn incremental_writes_are_routed_by_projection_effect() {
        let source = include_str!("index/write.rs");
        assert!(source.contains("ThreadIndexMutation::Unchanged => self.touch"));
        assert!(source.contains("apply_native_agent"));
        assert!(source.contains("apply_metadata"));
        assert!(source.contains("QueryBuilder::<Sqlite>"));
        assert!(!source.contains("CASE WHEN ? THEN ? ELSE"));
        assert!(!source.contains("mutation.agent_sequence()"));
        assert!(!source.contains("mutation.resume_config()"));
        assert!(source.contains("transcript_total_turns = transcript_total_turns + 1"));
    }

    #[test]
    fn live_transcript_window_reads_only_one_checkpoint() {
        let source = include_str!("index.rs")
            .split("pub(crate) async fn transcript_scan_plan")
            .nth(1)
            .expect("live transcript plan")
            .split("pub(crate) async fn latest_agent_event_sequence")
            .next()
            .expect("live transcript plan body");

        assert!(source.contains("transcript_total_turns, last_agent_sequence"));
        assert!(source.contains("ordinal = ?"));
        assert!(!source.contains("read_projection"));
        assert!(!source.contains("ORDER BY ordinal"));
        assert!(!source.contains("collect::<"));
    }
}
