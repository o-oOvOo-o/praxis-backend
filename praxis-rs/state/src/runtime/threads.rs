use super::*;
use crate::model::serialize_token_usage_info;

mod filters;
mod rollout_items;
mod source;
mod spawn_edges;
#[cfg(test)]
mod tests;

pub(crate) use filters::{push_thread_filters, push_thread_order_and_limit};
#[cfg(test)]
use filters::{sqlite_like_escape, thread_search_fts_query};
use rollout_items::{extract_dynamic_tools, extract_memory_mode};
use source::thread_source_columns_from_source_str;

impl StateRuntime {
    pub(crate) async fn backfill_thread_source_columns(&self) -> anyhow::Result<()> {
        let rows = sqlx::query(
            r#"
SELECT id, source, agent_base_name, agent_title, agent_display_name
FROM threads
WHERE source_kind IS NULL OR source_kind = ''
            "#,
        )
        .fetch_all(self.pool.as_ref())
        .await?;

        for row in rows {
            let id: String = row.try_get("id")?;
            let source: String = row.try_get("source")?;
            let agent_base_name: Option<String> = row.try_get("agent_base_name")?;
            let agent_title: Option<String> = row.try_get("agent_title")?;
            let agent_display_name: Option<String> = row.try_get("agent_display_name")?;
            let columns = thread_source_columns_from_source_str(
                &source,
                agent_base_name.as_deref(),
                agent_title.as_deref(),
                agent_display_name.as_deref(),
            );
            sqlx::query(
                r#"
UPDATE threads
SET
    source_kind = ?,
    subagent_kind = ?,
    subagent_parent_thread_id = ?,
    subagent_depth = ?,
    subagent_agent_base_name = ?,
    subagent_agent_title = ?,
    subagent_agent_display_name = ?
WHERE id = ?
                "#,
            )
            .bind(columns.source_kind)
            .bind(columns.subagent_kind)
            .bind(columns.subagent_parent_thread_id.as_deref())
            .bind(columns.subagent_depth)
            .bind(columns.subagent_agent_base_name.as_deref())
            .bind(columns.subagent_agent_title.as_deref())
            .bind(columns.subagent_agent_display_name.as_deref())
            .bind(id)
            .execute(self.pool.as_ref())
            .await?;
        }

        Ok(())
    }

    pub async fn set_thread_name(&self, id: ThreadId, name: &str) -> anyhow::Result<()> {
        sqlx::query(
            r#"
INSERT INTO thread_names (thread_id, name, updated_at)
VALUES (?, ?, ?)
ON CONFLICT(thread_id) DO UPDATE SET
    name = excluded.name,
    updated_at = excluded.updated_at
            "#,
        )
        .bind(id.to_string())
        .bind(name)
        .bind(Utc::now().timestamp())
        .execute(self.pool.as_ref())
        .await?;
        Ok(())
    }

    pub async fn get_thread_names(
        &self,
        ids: &std::collections::HashSet<ThreadId>,
    ) -> anyhow::Result<std::collections::HashMap<ThreadId, String>> {
        if ids.is_empty() {
            return Ok(std::collections::HashMap::new());
        }

        let mut builder = QueryBuilder::<Sqlite>::new(
            "SELECT thread_id, name FROM thread_names WHERE thread_id IN (",
        );
        let mut separated = builder.separated(", ");
        for id in ids {
            separated.push_bind(id.to_string());
        }
        separated.push_unseparated(")");

        let rows = builder.build().fetch_all(self.pool.as_ref()).await?;
        let mut names = std::collections::HashMap::with_capacity(rows.len());
        for row in rows {
            let id: String = row.try_get("thread_id")?;
            let Ok(thread_id) = ThreadId::from_string(&id) else {
                continue;
            };
            names.insert(thread_id, row.try_get("name")?);
        }
        Ok(names)
    }

    pub async fn get_threads(
        &self,
        ids: &std::collections::HashSet<ThreadId>,
    ) -> anyhow::Result<std::collections::HashMap<ThreadId, crate::ThreadMetadata>> {
        if ids.is_empty() {
            return Ok(std::collections::HashMap::new());
        }

        let mut builder = QueryBuilder::<Sqlite>::new(
            r#"
SELECT
    id,
    rollout_path,
    created_at,
    updated_at,
    source,
    agent_base_name,
    agent_title,
    agent_display_name,
    agent_role,
    agent_path,
    model_provider,
    model,
    reasoning_effort,
    cwd,
    cli_version,
    title,
    sandbox_policy,
    approval_mode,
    tokens_used,
    token_usage_info_json,
    session_summary,
    total_cost_micros,
    last_cost_micros,
    selfwork_plan_path,
    first_user_message,
    archived_at,
    git_sha,
    git_branch,
    git_origin_url
FROM threads WHERE id IN ("#,
        );
        let mut separated = builder.separated(", ");
        for id in ids {
            separated.push_bind(id.to_string());
        }
        separated.push_unseparated(")");

        let rows = builder.build().fetch_all(self.pool.as_ref()).await?;
        let mut threads = std::collections::HashMap::with_capacity(rows.len());
        for row in rows {
            let thread_row = ThreadRow::try_from_row(&row)?;
            let metadata = crate::ThreadMetadata::try_from(thread_row)?;
            threads.insert(metadata.id, metadata);
        }
        Ok(threads)
    }

    pub async fn get_thread(&self, id: ThreadId) -> anyhow::Result<Option<crate::ThreadMetadata>> {
        let row = sqlx::query(
            r#"
SELECT
    id,
    rollout_path,
    created_at,
    updated_at,
    source,
    agent_base_name,
    agent_title,
    agent_display_name,
    agent_role,
    agent_path,
    model_provider,
    model,
    reasoning_effort,
    cwd,
    cli_version,
    title,
    sandbox_policy,
    approval_mode,
    tokens_used,
    token_usage_info_json,
    session_summary,
    total_cost_micros,
    last_cost_micros,
    selfwork_plan_path,
    first_user_message,
    archived_at,
    git_sha,
    git_branch,
    git_origin_url
FROM threads
WHERE id = ?
            "#,
        )
        .bind(id.to_string())
        .fetch_optional(self.pool.as_ref())
        .await?;
        row.map(|row| ThreadRow::try_from_row(&row).and_then(ThreadMetadata::try_from))
            .transpose()
    }

    pub async fn get_thread_memory_mode(&self, id: ThreadId) -> anyhow::Result<Option<String>> {
        let row = sqlx::query("SELECT memory_mode FROM threads WHERE id = ?")
            .bind(id.to_string())
            .fetch_optional(self.pool.as_ref())
            .await?;
        Ok(row.and_then(|row| row.try_get("memory_mode").ok()))
    }

    /// Get dynamic tools for a thread, if present.
    pub async fn get_dynamic_tools(
        &self,
        thread_id: ThreadId,
    ) -> anyhow::Result<Option<Vec<DynamicToolSpec>>> {
        let rows = sqlx::query(
            r#"
SELECT name, description, input_schema, defer_loading
FROM thread_dynamic_tools
WHERE thread_id = ?
ORDER BY position ASC
            "#,
        )
        .bind(thread_id.to_string())
        .fetch_all(self.pool.as_ref())
        .await?;
        if rows.is_empty() {
            return Ok(None);
        }
        let mut tools = Vec::with_capacity(rows.len());
        for row in rows {
            let input_schema: String = row.try_get("input_schema")?;
            let input_schema = serde_json::from_str::<Value>(input_schema.as_str())?;
            tools.push(DynamicToolSpec {
                name: row.try_get("name")?,
                description: row.try_get("description")?,
                input_schema,
                defer_loading: row.try_get("defer_loading")?,
            });
        }
        Ok(Some(tools))
    }

    /// Find a rollout path by thread id using the underlying database.
    pub async fn find_rollout_path_by_id(
        &self,
        id: ThreadId,
        archived_only: Option<bool>,
    ) -> anyhow::Result<Option<PathBuf>> {
        let mut builder =
            QueryBuilder::<Sqlite>::new("SELECT rollout_path FROM threads WHERE id = ");
        builder.push_bind(id.to_string());
        match archived_only {
            Some(true) => {
                builder.push(" AND archived = 1");
            }
            Some(false) => {
                builder.push(" AND archived = 0");
            }
            None => {}
        }
        let row = builder.build().fetch_optional(self.pool.as_ref()).await?;
        Ok(row
            .and_then(|r| r.try_get::<String, _>("rollout_path").ok())
            .map(PathBuf::from))
    }

    /// List threads using the underlying database.
    #[allow(clippy::too_many_arguments)]
    pub async fn list_threads(
        &self,
        page_size: usize,
        anchor: Option<&crate::Anchor>,
        sort_key: crate::SortKey,
        allowed_sources: &[String],
        source_kinds: Option<&[crate::ThreadSourceKind]>,
        model_providers: Option<&[String]>,
        archived_only: bool,
        cwd: Option<&str>,
        search_term: Option<&str>,
    ) -> anyhow::Result<crate::ThreadsPage> {
        let limit = page_size.saturating_add(1);

        let mut builder = QueryBuilder::<Sqlite>::new(
            r#"
SELECT
    id,
    rollout_path,
    created_at,
    updated_at,
    source,
    agent_base_name,
    agent_title,
    agent_display_name,
    agent_role,
    agent_path,
    model_provider,
    model,
    reasoning_effort,
    cwd,
    cli_version,
    title,
    sandbox_policy,
    approval_mode,
    tokens_used,
    token_usage_info_json,
    session_summary,
    total_cost_micros,
    last_cost_micros,
    selfwork_plan_path,
    first_user_message,
    archived_at,
    git_sha,
    git_branch,
    git_origin_url
FROM threads
            "#,
        );
        push_thread_filters(
            &mut builder,
            archived_only,
            allowed_sources,
            source_kinds,
            model_providers,
            anchor,
            sort_key,
            cwd,
            search_term,
        );
        push_thread_order_and_limit(&mut builder, sort_key, limit);

        let rows = builder.build().fetch_all(self.pool.as_ref()).await?;
        let mut items = rows
            .into_iter()
            .map(|row| ThreadRow::try_from_row(&row).and_then(ThreadMetadata::try_from))
            .collect::<Result<Vec<_>, _>>()?;
        let num_scanned_rows = items.len();
        let next_anchor = if items.len() > page_size {
            items.pop();
            items
                .last()
                .and_then(|item| anchor_from_item(item, sort_key))
        } else {
            None
        };
        Ok(ThreadsPage {
            items,
            next_anchor,
            num_scanned_rows,
        })
    }

    /// List thread ids using the underlying database (no rollout scanning).
    pub async fn list_thread_ids(
        &self,
        limit: usize,
        anchor: Option<&crate::Anchor>,
        sort_key: crate::SortKey,
        allowed_sources: &[String],
        model_providers: Option<&[String]>,
        archived_only: bool,
    ) -> anyhow::Result<Vec<ThreadId>> {
        let mut builder = QueryBuilder::<Sqlite>::new("SELECT id FROM threads");
        push_thread_filters(
            &mut builder,
            archived_only,
            allowed_sources,
            /*source_kinds*/ None,
            model_providers,
            anchor,
            sort_key,
            /*cwd*/ None,
            /*search_term*/ None,
        );
        push_thread_order_and_limit(&mut builder, sort_key, limit);

        let rows = builder.build().fetch_all(self.pool.as_ref()).await?;
        rows.into_iter()
            .map(|row| {
                let id: String = row.try_get("id")?;
                Ok(ThreadId::try_from(id)?)
            })
            .collect()
    }

    /// Insert or replace thread metadata directly.
    pub async fn upsert_thread(&self, metadata: &crate::ThreadMetadata) -> anyhow::Result<()> {
        self.upsert_thread_with_creation_memory_mode(metadata, /*creation_memory_mode*/ None)
            .await
    }

    pub async fn insert_thread_if_absent(
        &self,
        metadata: &crate::ThreadMetadata,
    ) -> anyhow::Result<bool> {
        let token_usage_info_json = serialize_token_usage_info(metadata.token_usage_info.as_ref())?;
        let source_columns = thread_source_columns_from_source_str(
            &metadata.source,
            metadata.agent_base_name.as_deref(),
            metadata.agent_title.as_deref(),
            metadata.agent_display_name.as_deref(),
        );
        let result = sqlx::query(
            r#"
INSERT INTO threads (
    id,
    rollout_path,
    created_at,
    updated_at,
    source,
    source_kind,
    subagent_kind,
    subagent_parent_thread_id,
    subagent_depth,
    subagent_agent_base_name,
    subagent_agent_title,
    subagent_agent_display_name,
    agent_base_name,
    agent_title,
    agent_display_name,
    agent_role,
    agent_path,
    model_provider,
    model,
    reasoning_effort,
    cwd,
    cli_version,
    title,
    sandbox_policy,
    approval_mode,
    tokens_used,
    token_usage_info_json,
    session_summary,
    total_cost_micros,
    last_cost_micros,
    first_user_message,
    archived,
    archived_at,
    git_sha,
    git_branch,
    git_origin_url,
    memory_mode,
    selfwork_plan_path
) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
ON CONFLICT(id) DO NOTHING
            "#,
        )
        .bind(metadata.id.to_string())
        .bind(metadata.rollout_path.display().to_string())
        .bind(datetime_to_epoch_seconds(metadata.created_at))
        .bind(datetime_to_epoch_seconds(metadata.updated_at))
        .bind(metadata.source.as_str())
        .bind(source_columns.source_kind)
        .bind(source_columns.subagent_kind)
        .bind(source_columns.subagent_parent_thread_id.as_deref())
        .bind(source_columns.subagent_depth)
        .bind(source_columns.subagent_agent_base_name.as_deref())
        .bind(source_columns.subagent_agent_title.as_deref())
        .bind(source_columns.subagent_agent_display_name.as_deref())
        .bind(metadata.agent_base_name.as_deref())
        .bind(metadata.agent_title.as_deref())
        .bind(metadata.agent_display_name.as_deref())
        .bind(metadata.agent_role.as_deref())
        .bind(metadata.agent_path.as_deref())
        .bind(metadata.model_provider.as_str())
        .bind(metadata.model.as_deref())
        .bind(
            metadata
                .reasoning_effort
                .as_ref()
                .map(crate::extract::enum_to_string),
        )
        .bind(metadata.cwd.display().to_string())
        .bind(metadata.cli_version.as_str())
        .bind(metadata.title.as_str())
        .bind(metadata.sandbox_policy.as_str())
        .bind(metadata.approval_mode.as_str())
        .bind(metadata.tokens_used)
        .bind(token_usage_info_json.as_deref())
        .bind(metadata.session_summary.as_deref())
        .bind(metadata.total_cost_micros)
        .bind(metadata.last_cost_micros)
        .bind(metadata.first_user_message.as_deref().unwrap_or_default())
        .bind(metadata.archived_at.is_some())
        .bind(metadata.archived_at.map(datetime_to_epoch_seconds))
        .bind(metadata.git_sha.as_deref())
        .bind(metadata.git_branch.as_deref())
        .bind(metadata.git_origin_url.as_deref())
        .bind("enabled")
        .bind(
            metadata
                .selfwork_plan_path
                .as_ref()
                .map(|path| path.display().to_string()),
        )
        .execute(self.pool.as_ref())
        .await?;
        self.insert_thread_spawn_edge_from_source_if_absent(metadata.id, metadata.source.as_str())
            .await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn set_thread_memory_mode(
        &self,
        thread_id: ThreadId,
        memory_mode: &str,
    ) -> anyhow::Result<bool> {
        let result = sqlx::query("UPDATE threads SET memory_mode = ? WHERE id = ?")
            .bind(memory_mode)
            .bind(thread_id.to_string())
            .execute(self.pool.as_ref())
            .await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn touch_thread_updated_at(
        &self,
        thread_id: ThreadId,
        updated_at: DateTime<Utc>,
    ) -> anyhow::Result<bool> {
        let result = sqlx::query("UPDATE threads SET updated_at = ? WHERE id = ?")
            .bind(datetime_to_epoch_seconds(updated_at))
            .bind(thread_id.to_string())
            .execute(self.pool.as_ref())
            .await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn update_thread_git_info(
        &self,
        thread_id: ThreadId,
        git_sha: Option<Option<&str>>,
        git_branch: Option<Option<&str>>,
        git_origin_url: Option<Option<&str>>,
    ) -> anyhow::Result<bool> {
        let result = sqlx::query(
            r#"
UPDATE threads
SET
    git_sha = CASE WHEN ? THEN ? ELSE git_sha END,
    git_branch = CASE WHEN ? THEN ? ELSE git_branch END,
    git_origin_url = CASE WHEN ? THEN ? ELSE git_origin_url END
WHERE id = ?
            "#,
        )
        .bind(git_sha.is_some())
        .bind(git_sha.flatten())
        .bind(git_branch.is_some())
        .bind(git_branch.flatten())
        .bind(git_origin_url.is_some())
        .bind(git_origin_url.flatten())
        .bind(thread_id.to_string())
        .execute(self.pool.as_ref())
        .await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn update_thread_selfwork_plan_path(
        &self,
        thread_id: ThreadId,
        selfwork_plan_path: Option<&std::path::Path>,
    ) -> anyhow::Result<bool> {
        let result = sqlx::query("UPDATE threads SET selfwork_plan_path = ? WHERE id = ?")
            .bind(selfwork_plan_path.map(|path| path.display().to_string()))
            .bind(thread_id.to_string())
            .execute(self.pool.as_ref())
            .await?;
        Ok(result.rows_affected() > 0)
    }

    async fn upsert_thread_with_creation_memory_mode(
        &self,
        metadata: &crate::ThreadMetadata,
        creation_memory_mode: Option<&str>,
    ) -> anyhow::Result<()> {
        let token_usage_info_json = serialize_token_usage_info(metadata.token_usage_info.as_ref())?;
        let source_columns = thread_source_columns_from_source_str(
            &metadata.source,
            metadata.agent_base_name.as_deref(),
            metadata.agent_title.as_deref(),
            metadata.agent_display_name.as_deref(),
        );
        sqlx::query(
            r#"
INSERT INTO threads (
    id,
    rollout_path,
    created_at,
    updated_at,
    source,
    source_kind,
    subagent_kind,
    subagent_parent_thread_id,
    subagent_depth,
    subagent_agent_base_name,
    subagent_agent_title,
    subagent_agent_display_name,
    agent_base_name,
    agent_title,
    agent_display_name,
    agent_role,
    agent_path,
    model_provider,
    model,
    reasoning_effort,
    cwd,
    cli_version,
    title,
    sandbox_policy,
    approval_mode,
    tokens_used,
    token_usage_info_json,
    session_summary,
    total_cost_micros,
    last_cost_micros,
    first_user_message,
    archived,
    archived_at,
    git_sha,
    git_branch,
    git_origin_url,
    memory_mode,
    selfwork_plan_path
) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
ON CONFLICT(id) DO UPDATE SET
    rollout_path = excluded.rollout_path,
    created_at = excluded.created_at,
    updated_at = excluded.updated_at,
    source = excluded.source,
    source_kind = excluded.source_kind,
    subagent_kind = excluded.subagent_kind,
    subagent_parent_thread_id = excluded.subagent_parent_thread_id,
    subagent_depth = excluded.subagent_depth,
    subagent_agent_base_name = excluded.subagent_agent_base_name,
    subagent_agent_title = excluded.subagent_agent_title,
    subagent_agent_display_name = excluded.subagent_agent_display_name,
    agent_base_name = excluded.agent_base_name,
    agent_title = excluded.agent_title,
    agent_display_name = excluded.agent_display_name,
    agent_role = excluded.agent_role,
    agent_path = excluded.agent_path,
    model_provider = excluded.model_provider,
    model = excluded.model,
    reasoning_effort = excluded.reasoning_effort,
    cwd = excluded.cwd,
    cli_version = excluded.cli_version,
    title = excluded.title,
    sandbox_policy = excluded.sandbox_policy,
    approval_mode = excluded.approval_mode,
    tokens_used = excluded.tokens_used,
    token_usage_info_json = excluded.token_usage_info_json,
    session_summary = excluded.session_summary,
    total_cost_micros = excluded.total_cost_micros,
    last_cost_micros = excluded.last_cost_micros,
    selfwork_plan_path = excluded.selfwork_plan_path,
    first_user_message = excluded.first_user_message,
    archived = excluded.archived,
    archived_at = excluded.archived_at,
    git_sha = excluded.git_sha,
    git_branch = excluded.git_branch,
    git_origin_url = excluded.git_origin_url
            "#,
        )
        .bind(metadata.id.to_string())
        .bind(metadata.rollout_path.display().to_string())
        .bind(datetime_to_epoch_seconds(metadata.created_at))
        .bind(datetime_to_epoch_seconds(metadata.updated_at))
        .bind(metadata.source.as_str())
        .bind(source_columns.source_kind)
        .bind(source_columns.subagent_kind)
        .bind(source_columns.subagent_parent_thread_id.as_deref())
        .bind(source_columns.subagent_depth)
        .bind(source_columns.subagent_agent_base_name.as_deref())
        .bind(source_columns.subagent_agent_title.as_deref())
        .bind(source_columns.subagent_agent_display_name.as_deref())
        .bind(metadata.agent_base_name.as_deref())
        .bind(metadata.agent_title.as_deref())
        .bind(metadata.agent_display_name.as_deref())
        .bind(metadata.agent_role.as_deref())
        .bind(metadata.agent_path.as_deref())
        .bind(metadata.model_provider.as_str())
        .bind(metadata.model.as_deref())
        .bind(
            metadata
                .reasoning_effort
                .as_ref()
                .map(crate::extract::enum_to_string),
        )
        .bind(metadata.cwd.display().to_string())
        .bind(metadata.cli_version.as_str())
        .bind(metadata.title.as_str())
        .bind(metadata.sandbox_policy.as_str())
        .bind(metadata.approval_mode.as_str())
        .bind(metadata.tokens_used)
        .bind(token_usage_info_json.as_deref())
        .bind(metadata.session_summary.as_deref())
        .bind(metadata.total_cost_micros)
        .bind(metadata.last_cost_micros)
        .bind(metadata.first_user_message.as_deref().unwrap_or_default())
        .bind(metadata.archived_at.is_some())
        .bind(metadata.archived_at.map(datetime_to_epoch_seconds))
        .bind(metadata.git_sha.as_deref())
        .bind(metadata.git_branch.as_deref())
        .bind(metadata.git_origin_url.as_deref())
        .bind(creation_memory_mode.unwrap_or("enabled"))
        .bind(
            metadata
                .selfwork_plan_path
                .as_ref()
                .map(|path| path.display().to_string()),
        )
        .execute(self.pool.as_ref())
        .await?;
        self.insert_thread_spawn_edge_from_source_if_absent(metadata.id, metadata.source.as_str())
            .await?;
        Ok(())
    }

    /// Persist dynamic tools for a thread if none have been stored yet.
    ///
    /// Dynamic tools are defined at thread start and should not change afterward.
    /// This only writes the first time we see tools for a given thread.
    pub async fn persist_dynamic_tools(
        &self,
        thread_id: ThreadId,
        tools: Option<&[DynamicToolSpec]>,
    ) -> anyhow::Result<()> {
        let Some(tools) = tools else {
            return Ok(());
        };
        if tools.is_empty() {
            return Ok(());
        }
        let thread_id = thread_id.to_string();
        let mut tx = self.pool.begin().await?;
        for (idx, tool) in tools.iter().enumerate() {
            let position = i64::try_from(idx).unwrap_or(i64::MAX);
            let input_schema = serde_json::to_string(&tool.input_schema)?;
            sqlx::query(
                r#"
INSERT INTO thread_dynamic_tools (
    thread_id,
    position,
    name,
    description,
    input_schema,
    defer_loading
) VALUES (?, ?, ?, ?, ?, ?)
ON CONFLICT(thread_id, position) DO NOTHING
                "#,
            )
            .bind(thread_id.as_str())
            .bind(position)
            .bind(tool.name.as_str())
            .bind(tool.description.as_str())
            .bind(input_schema)
            .bind(tool.defer_loading)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    /// Apply rollout items incrementally using the underlying database.
    pub async fn apply_rollout_items(
        &self,
        builder: &ThreadMetadataBuilder,
        items: &[RolloutItem],
        new_thread_memory_mode: Option<&str>,
        updated_at_override: Option<DateTime<Utc>>,
    ) -> anyhow::Result<()> {
        if items.is_empty() {
            return Ok(());
        }
        let existing_metadata = self.get_thread(builder.id).await?;
        let mut metadata = existing_metadata
            .clone()
            .unwrap_or_else(|| builder.build(&self.default_provider));
        metadata.rollout_path = builder.rollout_path.clone();
        for item in items {
            apply_rollout_item(&mut metadata, item, &self.default_provider);
        }
        if let Some(existing_metadata) = existing_metadata.as_ref() {
            metadata.prefer_existing_git_info(existing_metadata);
        }
        let updated_at = match updated_at_override {
            Some(updated_at) => Some(updated_at),
            None => file_modified_time_utc(builder.rollout_path.as_path()).await,
        };
        if let Some(updated_at) = updated_at {
            metadata.updated_at = updated_at;
        }
        // Keep the thread upsert before dynamic tools to satisfy the foreign key constraint:
        // thread_dynamic_tools.thread_id -> threads.id.
        let upsert_result = if existing_metadata.is_none() {
            self.upsert_thread_with_creation_memory_mode(&metadata, new_thread_memory_mode)
                .await
        } else {
            self.upsert_thread(&metadata).await
        };
        upsert_result?;
        if let Some(memory_mode) = extract_memory_mode(items)
            && let Err(err) = self
                .set_thread_memory_mode(builder.id, memory_mode.as_str())
                .await
        {
            return Err(err);
        }
        let dynamic_tools = extract_dynamic_tools(items);
        if let Some(dynamic_tools) = dynamic_tools
            && let Err(err) = self
                .persist_dynamic_tools(builder.id, dynamic_tools.as_deref())
                .await
        {
            return Err(err);
        }
        Ok(())
    }

    /// Mark a thread as archived using the underlying database.
    pub async fn mark_archived(
        &self,
        thread_id: ThreadId,
        rollout_path: &Path,
        archived_at: DateTime<Utc>,
    ) -> anyhow::Result<()> {
        let Some(mut metadata) = self.get_thread(thread_id).await? else {
            return Ok(());
        };
        metadata.archived_at = Some(archived_at);
        metadata.rollout_path = rollout_path.to_path_buf();
        if let Some(updated_at) = file_modified_time_utc(rollout_path).await {
            metadata.updated_at = updated_at;
        }
        if metadata.id != thread_id {
            warn!(
                "thread id mismatch during archive: expected {thread_id}, got {}",
                metadata.id
            );
        }
        self.upsert_thread(&metadata).await
    }

    /// Mark a thread as unarchived using the underlying database.
    pub async fn mark_unarchived(
        &self,
        thread_id: ThreadId,
        rollout_path: &Path,
    ) -> anyhow::Result<()> {
        let Some(mut metadata) = self.get_thread(thread_id).await? else {
            return Ok(());
        };
        metadata.archived_at = None;
        metadata.rollout_path = rollout_path.to_path_buf();
        if let Some(updated_at) = file_modified_time_utc(rollout_path).await {
            metadata.updated_at = updated_at;
        }
        if metadata.id != thread_id {
            warn!(
                "thread id mismatch during unarchive: expected {thread_id}, got {}",
                metadata.id
            );
        }
        self.upsert_thread(&metadata).await
    }

    /// Delete a thread metadata row by id.
    pub async fn delete_thread(&self, thread_id: ThreadId) -> anyhow::Result<u64> {
        let result = sqlx::query("DELETE FROM threads WHERE id = ?")
            .bind(thread_id.to_string())
            .execute(self.pool.as_ref())
            .await?;
        Ok(result.rows_affected())
    }
}
