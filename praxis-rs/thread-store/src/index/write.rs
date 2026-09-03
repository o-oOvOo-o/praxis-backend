use super::CURRENT_PROJECTION_GENERATION;
use super::ThreadIndex;
use super::revision_to_i64;
use crate::ThreadStoreError;
use crate::projection::ThreadIndexMutation;
use crate::projection::ThreadIndexProjection;
use praxis_thread_store_contracts::ThreadEventBody;
use praxis_thread_store_contracts::ThreadEventEnvelope;
use praxis_thread_store_contracts::ThreadRevision;
use sqlx::QueryBuilder;
use sqlx::Sqlite;

struct WriteCursor {
    thread_id: String,
    revision: i64,
    expected_revision: i64,
    updated_at: i64,
}

impl WriteCursor {
    fn new(event: &ThreadEventEnvelope) -> Result<Self, ThreadStoreError> {
        Ok(Self {
            thread_id: event.thread_id.to_string(),
            revision: revision_to_i64(event.revision)?,
            expected_revision: revision_to_i64(ThreadRevision::new(
                event.revision.get().saturating_sub(1),
            ))?,
            updated_at: event.recorded_at_unix_ms,
        })
    }
}

impl ThreadIndex {
    pub(crate) async fn apply_all(
        &self,
        events: &[ThreadEventEnvelope],
    ) -> Result<bool, ThreadStoreError> {
        let mut index = 0;
        while index < events.len() {
            let ThreadIndexMutation::NativeAgent {
                sequence,
                turn_started: false,
            } = ThreadIndexMutation::from_event(&events[index])
            else {
                if !self.apply(&events[index]).await? {
                    return Ok(false);
                }
                index += 1;
                continue;
            };
            let first = WriteCursor::new(&events[index])?;
            let mut last = WriteCursor::new(&events[index])?;
            let mut max_sequence = sequence;
            index += 1;
            while index < events.len() {
                let ThreadIndexMutation::NativeAgent {
                    sequence,
                    turn_started: false,
                } = ThreadIndexMutation::from_event(&events[index])
                else {
                    break;
                };
                last = WriteCursor::new(&events[index])?;
                max_sequence = max_sequence.max(sequence);
                index += 1;
            }
            if !self
                .apply_native_agent_run(&first, &last, max_sequence)
                .await?
            {
                return Ok(false);
            }
        }
        Ok(true)
    }

    pub(crate) async fn apply(
        &self,
        event: &ThreadEventEnvelope,
    ) -> Result<bool, ThreadStoreError> {
        if matches!(event.body, ThreadEventBody::ThreadCreated { .. }) {
            if let Some(projection) = ThreadIndexProjection::from_created(event) {
                self.replace(&projection).await?;
            }
            return Ok(true);
        }
        let cursor = WriteCursor::new(event)?;
        match ThreadIndexMutation::from_event(event) {
            ThreadIndexMutation::NativeAgent {
                sequence,
                turn_started,
            } => {
                self.apply_native_agent(&cursor, sequence, turn_started)
                    .await
            }
            ThreadIndexMutation::Unchanged => self.touch(&cursor).await,
            mutation => self.apply_metadata(&cursor, mutation).await,
        }
    }

    async fn touch(&self, cursor: &WriteCursor) -> Result<bool, ThreadStoreError> {
        let result = sqlx::query(
            "UPDATE thread_projection SET revision = ?, updated_at = ?
             WHERE thread_id = ? AND revision = ? AND projection_generation = ?",
        )
        .bind(cursor.revision)
        .bind(cursor.updated_at)
        .bind(&cursor.thread_id)
        .bind(cursor.expected_revision)
        .bind(CURRENT_PROJECTION_GENERATION)
        .execute(&self.shared.pool)
        .await?;
        Ok(result.rows_affected() == 1)
    }

    async fn apply_native_agent(
        &self,
        cursor: &WriteCursor,
        sequence: u64,
        turn_started: bool,
    ) -> Result<bool, ThreadStoreError> {
        let sequence = i64::try_from(sequence).map_err(|_| ThreadStoreError::RevisionOverflow)?;
        if !turn_started {
            let result = sqlx::query(
                "UPDATE thread_projection SET revision = ?, updated_at = ?,
                    last_agent_sequence = MAX(last_agent_sequence, ?)
                 WHERE thread_id = ? AND revision = ? AND projection_generation = ?",
            )
            .bind(cursor.revision)
            .bind(cursor.updated_at)
            .bind(sequence)
            .bind(&cursor.thread_id)
            .bind(cursor.expected_revision)
            .bind(CURRENT_PROJECTION_GENERATION)
            .execute(&self.shared.pool)
            .await?;
            return Ok(result.rows_affected() == 1);
        }
        self.apply_turn_start(cursor, sequence).await
    }

    async fn apply_native_agent_run(
        &self,
        first: &WriteCursor,
        last: &WriteCursor,
        max_sequence: u64,
    ) -> Result<bool, ThreadStoreError> {
        if first.thread_id != last.thread_id {
            return Ok(false);
        }
        let sequence =
            i64::try_from(max_sequence).map_err(|_| ThreadStoreError::RevisionOverflow)?;
        let result = sqlx::query(
            "UPDATE thread_projection SET revision = ?, updated_at = ?,
                last_agent_sequence = MAX(last_agent_sequence, ?)
             WHERE thread_id = ? AND revision = ? AND projection_generation = ?",
        )
        .bind(last.revision)
        .bind(last.updated_at)
        .bind(sequence)
        .bind(&first.thread_id)
        .bind(first.expected_revision)
        .bind(CURRENT_PROJECTION_GENERATION)
        .execute(&self.shared.pool)
        .await?;
        Ok(result.rows_affected() == 1)
    }

    async fn apply_turn_start(
        &self,
        cursor: &WriteCursor,
        sequence: i64,
    ) -> Result<bool, ThreadStoreError> {
        let mut transaction = self.shared.pool.begin().await?;
        let frontier = sqlx::query_as::<_, (i64, i64)>(
            "SELECT transcript_total_turns, last_agent_sequence
             FROM thread_projection
             WHERE thread_id = ? AND revision = ? AND projection_generation = ?",
        )
        .bind(&cursor.thread_id)
        .bind(cursor.expected_revision)
        .bind(CURRENT_PROJECTION_GENERATION)
        .fetch_optional(&mut *transaction)
        .await?;
        let Some((ordinal, previous_sequence)) = frontier else {
            transaction.rollback().await?;
            return Ok(false);
        };
        let result = sqlx::query(
            "UPDATE thread_projection SET revision = ?, updated_at = ?,
                last_agent_sequence = MAX(last_agent_sequence, ?),
                transcript_total_turns = transcript_total_turns + 1
             WHERE thread_id = ? AND revision = ? AND projection_generation = ?",
        )
        .bind(cursor.revision)
        .bind(cursor.updated_at)
        .bind(sequence)
        .bind(&cursor.thread_id)
        .bind(cursor.expected_revision)
        .bind(CURRENT_PROJECTION_GENERATION)
        .execute(&mut *transaction)
        .await?;
        if result.rows_affected() != 1 {
            transaction.rollback().await?;
            return Ok(false);
        }
        self.insert_turn_checkpoint(&mut transaction, cursor, ordinal, previous_sequence)
            .await?;
        transaction.commit().await?;
        Ok(true)
    }

    async fn insert_turn_checkpoint(
        &self,
        transaction: &mut sqlx::Transaction<'_, Sqlite>,
        cursor: &WriteCursor,
        ordinal: i64,
        previous_sequence: i64,
    ) -> Result<(), ThreadStoreError> {
        sqlx::query(
            "INSERT INTO thread_turn_checkpoint
                (thread_id, ordinal, scan_after_revision, previous_agent_sequence)
             VALUES (?, ?, ?, ?)",
        )
        .bind(&cursor.thread_id)
        .bind(ordinal)
        .bind(cursor.expected_revision)
        .bind((previous_sequence != 0).then_some(previous_sequence))
        .execute(&mut **transaction)
        .await?;
        sqlx::query(
            "DELETE FROM thread_turn_checkpoint
             WHERE thread_id = ? AND ordinal < (
                SELECT COALESCE(MAX(ordinal), 0) - 255
                FROM thread_turn_checkpoint WHERE thread_id = ?
             )",
        )
        .bind(&cursor.thread_id)
        .bind(&cursor.thread_id)
        .execute(&mut **transaction)
        .await?;
        Ok(())
    }

    async fn apply_metadata(
        &self,
        cursor: &WriteCursor,
        mutation: ThreadIndexMutation<'_>,
    ) -> Result<bool, ThreadStoreError> {
        let mut query = QueryBuilder::<Sqlite>::new("UPDATE thread_projection SET revision = ");
        query
            .push_bind(cursor.revision)
            .push(", updated_at = ")
            .push_bind(cursor.updated_at);
        match mutation {
            ThreadIndexMutation::Name(value) => {
                query.push(", name = ").push_bind(value);
            }
            ThreadIndexMutation::Summary(value) => {
                query.push(", summary = ").push_bind(value);
            }
            ThreadIndexMutation::Archived(value) => {
                query.push(", archived = ").push_bind(value);
            }
            ThreadIndexMutation::Workspace(value) => {
                query.push(", workspace = ").push_bind(value);
            }
            ThreadIndexMutation::Preview(value) => {
                query.push(", preview = ").push_bind(value);
            }
            ThreadIndexMutation::PreviewSnapshot(preview, first_user_message) => {
                query
                    .push(", preview = ")
                    .push_bind(preview.as_deref())
                    .push(", first_user_message = ")
                    .push_bind(first_user_message.as_deref());
            }
            ThreadIndexMutation::UserMessage(value) => {
                query
                    .push(", preview = ")
                    .push_bind(value)
                    .push(", first_user_message = COALESCE(first_user_message, ")
                    .push_bind(value)
                    .push(")");
            }
            ThreadIndexMutation::Cost(value) => {
                query.push(", last_cost_micros = ").push_bind(value);
                if let Some(value) = value {
                    query
                        .push(", total_cost_micros = COALESCE(total_cost_micros, 0) + ")
                        .push_bind(value);
                }
            }
            ThreadIndexMutation::ResumeConfig(model, provider, effort) => {
                query
                    .push(", model = ")
                    .push_bind(model.as_deref())
                    .push(", model_provider = ")
                    .push_bind(provider.as_deref())
                    .push(", reasoning_effort = ")
                    .push_bind(effort.as_deref());
            }
            ThreadIndexMutation::ModelContextCheckpoint(revision) => {
                let revision = revision.map(revision_to_i64).transpose()?;
                query
                    .push(", model_context_checkpoint_revision = ")
                    .push_bind(revision);
            }
            ThreadIndexMutation::DynamicTools(tools) => {
                query.push(", dynamic_tools_digest = ").push_bind(
                    crate::projection::dynamic_tools_digest(tools)
                        .as_bytes()
                        .to_vec(),
                );
            }
            ThreadIndexMutation::AgentEventMetadataGeneration(generation) => {
                query
                    .push(", agent_event_metadata_generation = ")
                    .push_bind(i64::from(generation));
            }
            ThreadIndexMutation::AgentEventTimeline(generation, created_at, updated_at) => {
                query
                    .push(", agent_event_metadata_generation = ")
                    .push_bind(i64::from(generation));
                if let Some(created_at) = created_at {
                    query.push(", created_at = ").push_bind(*created_at);
                }
                if let Some(updated_at) = updated_at {
                    query.push(", updated_at = ").push_bind(*updated_at);
                }
            }
            ThreadIndexMutation::Unchanged | ThreadIndexMutation::NativeAgent { .. } => {
                return Err(ThreadStoreError::Worker(
                    "index mutation routed to the wrong writer".to_string(),
                ));
            }
        }
        query
            .push(" WHERE thread_id = ")
            .push_bind(&cursor.thread_id)
            .push(" AND revision = ")
            .push_bind(cursor.expected_revision)
            .push(" AND projection_generation = ")
            .push_bind(CURRENT_PROJECTION_GENERATION);
        let result = query.build().execute(&self.shared.pool).await?;
        Ok(result.rows_affected() == 1)
    }
}
