use super::*;
use crate::model::serialize_token_usage_info;
use praxis_protocol::protocol::SessionSource;
use praxis_protocol::protocol::SubAgentSource;

#[derive(Debug, Clone)]
struct ThreadSourceColumns {
    source_kind: &'static str,
    subagent_kind: Option<&'static str>,
    subagent_parent_thread_id: Option<String>,
    subagent_depth: Option<i64>,
    subagent_agent_base_name: Option<String>,
    subagent_agent_title: Option<String>,
    subagent_agent_display_name: Option<String>,
}

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

    /// Persist or replace the directional parent-child edge for a spawned thread.
    pub async fn upsert_thread_spawn_edge(
        &self,
        parent_thread_id: ThreadId,
        child_thread_id: ThreadId,
        status: crate::DirectionalThreadSpawnEdgeStatus,
    ) -> anyhow::Result<()> {
        sqlx::query(
            r#"
INSERT INTO thread_spawn_edges (
    parent_thread_id,
    child_thread_id,
    status
) VALUES (?, ?, ?)
ON CONFLICT(child_thread_id) DO UPDATE SET
    parent_thread_id = excluded.parent_thread_id,
    status = excluded.status
            "#,
        )
        .bind(parent_thread_id.to_string())
        .bind(child_thread_id.to_string())
        .bind(status.as_ref())
        .execute(self.pool.as_ref())
        .await?;
        Ok(())
    }

    /// Update the persisted lifecycle status of a spawned thread's incoming edge.
    pub async fn set_thread_spawn_edge_status(
        &self,
        child_thread_id: ThreadId,
        status: crate::DirectionalThreadSpawnEdgeStatus,
    ) -> anyhow::Result<()> {
        sqlx::query("UPDATE thread_spawn_edges SET status = ? WHERE child_thread_id = ?")
            .bind(status.as_ref())
            .bind(child_thread_id.to_string())
            .execute(self.pool.as_ref())
            .await?;
        Ok(())
    }

    /// List direct spawned children of `parent_thread_id` whose edge matches `status`.
    pub async fn list_thread_spawn_children_with_status(
        &self,
        parent_thread_id: ThreadId,
        status: crate::DirectionalThreadSpawnEdgeStatus,
    ) -> anyhow::Result<Vec<ThreadId>> {
        self.list_thread_spawn_children_matching(parent_thread_id, Some(status))
            .await
    }

    /// List spawned descendants of `root_thread_id` whose edges match `status`.
    ///
    /// Descendants are returned breadth-first by depth, then by thread id for stable ordering.
    pub async fn list_thread_spawn_descendants_with_status(
        &self,
        root_thread_id: ThreadId,
        status: crate::DirectionalThreadSpawnEdgeStatus,
    ) -> anyhow::Result<Vec<ThreadId>> {
        self.list_thread_spawn_descendants_matching(root_thread_id, Some(status))
            .await
    }

    /// Find a direct spawned child of `parent_thread_id` by canonical agent path.
    pub async fn find_thread_spawn_child_by_path(
        &self,
        parent_thread_id: ThreadId,
        agent_path: &str,
    ) -> anyhow::Result<Option<ThreadId>> {
        let rows = sqlx::query(
            r#"
SELECT threads.id
FROM thread_spawn_edges
JOIN threads ON threads.id = thread_spawn_edges.child_thread_id
WHERE thread_spawn_edges.parent_thread_id = ?
  AND threads.agent_path = ?
ORDER BY threads.id
LIMIT 2
            "#,
        )
        .bind(parent_thread_id.to_string())
        .bind(agent_path)
        .fetch_all(self.pool.as_ref())
        .await?;
        one_thread_id_from_rows(rows, agent_path)
    }

    /// Find a spawned descendant of `root_thread_id` by canonical agent path.
    pub async fn find_thread_spawn_descendant_by_path(
        &self,
        root_thread_id: ThreadId,
        agent_path: &str,
    ) -> anyhow::Result<Option<ThreadId>> {
        let rows = sqlx::query(
            r#"
WITH RECURSIVE subtree(child_thread_id) AS (
    SELECT child_thread_id
    FROM thread_spawn_edges
    WHERE parent_thread_id = ?
    UNION ALL
    SELECT edge.child_thread_id
    FROM thread_spawn_edges AS edge
    JOIN subtree ON edge.parent_thread_id = subtree.child_thread_id
)
SELECT threads.id
FROM subtree
JOIN threads ON threads.id = subtree.child_thread_id
WHERE threads.agent_path = ?
ORDER BY threads.id
LIMIT 2
            "#,
        )
        .bind(root_thread_id.to_string())
        .bind(agent_path)
        .fetch_all(self.pool.as_ref())
        .await?;
        one_thread_id_from_rows(rows, agent_path)
    }

    async fn list_thread_spawn_children_matching(
        &self,
        parent_thread_id: ThreadId,
        status: Option<crate::DirectionalThreadSpawnEdgeStatus>,
    ) -> anyhow::Result<Vec<ThreadId>> {
        let mut query = String::from(
            "SELECT child_thread_id FROM thread_spawn_edges WHERE parent_thread_id = ?",
        );
        if status.is_some() {
            query.push_str(" AND status = ?");
        }
        query.push_str(" ORDER BY child_thread_id");

        let mut sql = sqlx::query(query.as_str()).bind(parent_thread_id.to_string());
        if let Some(status) = status {
            sql = sql.bind(status.to_string());
        }

        let rows = sql.fetch_all(self.pool.as_ref()).await?;
        rows.into_iter()
            .map(|row| {
                ThreadId::try_from(row.try_get::<String, _>("child_thread_id")?).map_err(Into::into)
            })
            .collect()
    }

    async fn list_thread_spawn_descendants_matching(
        &self,
        root_thread_id: ThreadId,
        status: Option<crate::DirectionalThreadSpawnEdgeStatus>,
    ) -> anyhow::Result<Vec<ThreadId>> {
        let status_filter = if status.is_some() {
            " AND status = ?"
        } else {
            ""
        };
        let query = format!(
            r#"
WITH RECURSIVE subtree(child_thread_id, depth) AS (
    SELECT child_thread_id, 1
    FROM thread_spawn_edges
    WHERE parent_thread_id = ?{status_filter}
    UNION ALL
    SELECT edge.child_thread_id, subtree.depth + 1
    FROM thread_spawn_edges AS edge
    JOIN subtree ON edge.parent_thread_id = subtree.child_thread_id
    WHERE 1 = 1{status_filter}
)
SELECT child_thread_id
FROM subtree
ORDER BY depth ASC, child_thread_id ASC
            "#
        );

        let mut sql = sqlx::query(query.as_str()).bind(root_thread_id.to_string());
        if let Some(status) = status {
            let status = status.to_string();
            sql = sql.bind(status.clone()).bind(status);
        }

        let rows = sql.fetch_all(self.pool.as_ref()).await?;
        rows.into_iter()
            .map(|row| {
                ThreadId::try_from(row.try_get::<String, _>("child_thread_id")?).map_err(Into::into)
            })
            .collect()
    }

    async fn insert_thread_spawn_edge_if_absent(
        &self,
        parent_thread_id: ThreadId,
        child_thread_id: ThreadId,
    ) -> anyhow::Result<()> {
        sqlx::query(
            r#"
INSERT INTO thread_spawn_edges (
    parent_thread_id,
    child_thread_id,
    status
) VALUES (?, ?, ?)
ON CONFLICT(child_thread_id) DO NOTHING
            "#,
        )
        .bind(parent_thread_id.to_string())
        .bind(child_thread_id.to_string())
        .bind(crate::DirectionalThreadSpawnEdgeStatus::Open.as_ref())
        .execute(self.pool.as_ref())
        .await?;
        Ok(())
    }

    async fn insert_thread_spawn_edge_from_source_if_absent(
        &self,
        child_thread_id: ThreadId,
        source: &str,
    ) -> anyhow::Result<()> {
        let Some(parent_thread_id) = thread_spawn_parent_thread_id_from_source_str(source) else {
            return Ok(());
        };
        self.insert_thread_spawn_edge_if_absent(parent_thread_id, child_thread_id)
            .await
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

fn one_thread_id_from_rows(
    rows: Vec<sqlx::sqlite::SqliteRow>,
    agent_path: &str,
) -> anyhow::Result<Option<ThreadId>> {
    let mut ids = rows
        .into_iter()
        .map(|row| {
            let id: String = row.try_get("id")?;
            ThreadId::try_from(id).map_err(anyhow::Error::from)
        })
        .collect::<Result<Vec<_>, _>>()?;
    match ids.len() {
        0 => Ok(None),
        1 => Ok(ids.pop()),
        _ => Err(anyhow::anyhow!(
            "multiple agents found for canonical path `{agent_path}`"
        )),
    }
}

pub(super) fn extract_dynamic_tools(items: &[RolloutItem]) -> Option<Option<Vec<DynamicToolSpec>>> {
    items.iter().find_map(|item| match item {
        RolloutItem::SessionMeta(meta_line) => Some(meta_line.meta.dynamic_tools.clone()),
        RolloutItem::ResponseItem(_)
        | RolloutItem::Compacted(_)
        | RolloutItem::TurnContext(_)
        | RolloutItem::EventMsg(_) => None,
    })
}

pub(super) fn extract_memory_mode(items: &[RolloutItem]) -> Option<String> {
    items.iter().rev().find_map(|item| match item {
        RolloutItem::SessionMeta(meta_line) => meta_line.meta.memory_mode.clone(),
        RolloutItem::ResponseItem(_)
        | RolloutItem::Compacted(_)
        | RolloutItem::TurnContext(_)
        | RolloutItem::EventMsg(_) => None,
    })
}

fn thread_search_fts_query(search_term: &str) -> Option<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    for ch in search_term.chars() {
        if ch.is_alphanumeric() {
            current.extend(ch.to_lowercase());
        } else if !current.is_empty() {
            tokens.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    if tokens.is_empty() {
        return None;
    }

    Some(
        tokens
            .into_iter()
            .map(|token| format!("{}*", token.replace('"', "\"\"")))
            .collect::<Vec<_>>()
            .join(" AND "),
    )
}

fn thread_spawn_parent_thread_id_from_source_str(source: &str) -> Option<ThreadId> {
    let parsed_source = serde_json::from_str(source)
        .or_else(|_| serde_json::from_value::<SessionSource>(Value::String(source.to_string())));
    match parsed_source.ok() {
        Some(SessionSource::SubAgent(praxis_protocol::protocol::SubAgentSource::ThreadSpawn {
            parent_thread_id,
            ..
        })) => Some(parent_thread_id),
        _ => None,
    }
}

fn thread_source_columns_from_source_str(
    source: &str,
    fallback_agent_base_name: Option<&str>,
    fallback_agent_title: Option<&str>,
    fallback_agent_display_name: Option<&str>,
) -> ThreadSourceColumns {
    let parsed_source = serde_json::from_str(source)
        .or_else(|_| serde_json::from_value::<SessionSource>(Value::String(source.to_string())))
        .unwrap_or(SessionSource::Unknown);
    thread_source_columns_from_source(
        parsed_source,
        fallback_agent_base_name,
        fallback_agent_title,
        fallback_agent_display_name,
    )
}

fn thread_source_columns_from_source(
    source: SessionSource,
    fallback_agent_base_name: Option<&str>,
    fallback_agent_title: Option<&str>,
    fallback_agent_display_name: Option<&str>,
) -> ThreadSourceColumns {
    match source {
        SessionSource::Cli => ThreadSourceColumns {
            source_kind: "cli",
            subagent_kind: None,
            subagent_parent_thread_id: None,
            subagent_depth: None,
            subagent_agent_base_name: None,
            subagent_agent_title: None,
            subagent_agent_display_name: None,
        },
        SessionSource::VSCode => ThreadSourceColumns {
            source_kind: "vscode",
            subagent_kind: None,
            subagent_parent_thread_id: None,
            subagent_depth: None,
            subagent_agent_base_name: None,
            subagent_agent_title: None,
            subagent_agent_display_name: None,
        },
        SessionSource::Exec => ThreadSourceColumns {
            source_kind: "exec",
            subagent_kind: None,
            subagent_parent_thread_id: None,
            subagent_depth: None,
            subagent_agent_base_name: None,
            subagent_agent_title: None,
            subagent_agent_display_name: None,
        },
        SessionSource::AppGateway => ThreadSourceColumns {
            source_kind: "app_gateway",
            subagent_kind: None,
            subagent_parent_thread_id: None,
            subagent_depth: None,
            subagent_agent_base_name: None,
            subagent_agent_title: None,
            subagent_agent_display_name: None,
        },
        SessionSource::Mcp => ThreadSourceColumns {
            source_kind: "mcp",
            subagent_kind: None,
            subagent_parent_thread_id: None,
            subagent_depth: None,
            subagent_agent_base_name: None,
            subagent_agent_title: None,
            subagent_agent_display_name: None,
        },
        SessionSource::Custom(_) => ThreadSourceColumns {
            source_kind: "custom",
            subagent_kind: None,
            subagent_parent_thread_id: None,
            subagent_depth: None,
            subagent_agent_base_name: None,
            subagent_agent_title: None,
            subagent_agent_display_name: None,
        },
        SessionSource::Unknown => ThreadSourceColumns {
            source_kind: "unknown",
            subagent_kind: None,
            subagent_parent_thread_id: None,
            subagent_depth: None,
            subagent_agent_base_name: None,
            subagent_agent_title: None,
            subagent_agent_display_name: None,
        },
        SessionSource::SubAgent(subagent) => match subagent {
            SubAgentSource::Review => ThreadSourceColumns {
                source_kind: "subagent",
                subagent_kind: Some("review"),
                subagent_parent_thread_id: None,
                subagent_depth: None,
                subagent_agent_base_name: fallback_agent_base_name.map(str::to_string),
                subagent_agent_title: fallback_agent_title.map(str::to_string),
                subagent_agent_display_name: fallback_agent_display_name.map(str::to_string),
            },
            SubAgentSource::Compact => ThreadSourceColumns {
                source_kind: "subagent",
                subagent_kind: Some("compact"),
                subagent_parent_thread_id: None,
                subagent_depth: None,
                subagent_agent_base_name: fallback_agent_base_name.map(str::to_string),
                subagent_agent_title: fallback_agent_title.map(str::to_string),
                subagent_agent_display_name: fallback_agent_display_name.map(str::to_string),
            },
            SubAgentSource::ThreadSpawn {
                parent_thread_id,
                depth,
                agent_base_name,
                agent_title,
                agent_display_name,
                ..
            } => ThreadSourceColumns {
                source_kind: "subagent",
                subagent_kind: Some("thread_spawn"),
                subagent_parent_thread_id: Some(parent_thread_id.to_string()),
                subagent_depth: Some(depth as i64),
                subagent_agent_base_name: agent_base_name
                    .or_else(|| fallback_agent_base_name.map(str::to_string)),
                subagent_agent_title: agent_title
                    .or_else(|| fallback_agent_title.map(str::to_string)),
                subagent_agent_display_name: agent_display_name
                    .or_else(|| fallback_agent_display_name.map(str::to_string)),
            },
            SubAgentSource::MemoryConsolidation => ThreadSourceColumns {
                source_kind: "subagent",
                subagent_kind: Some("memory_consolidation"),
                subagent_parent_thread_id: None,
                subagent_depth: None,
                subagent_agent_base_name: fallback_agent_base_name
                    .map(str::to_string)
                    .or_else(|| Some("Morpheus".to_string())),
                subagent_agent_title: fallback_agent_title.map(str::to_string),
                subagent_agent_display_name: fallback_agent_display_name
                    .map(str::to_string)
                    .or_else(|| Some("Morpheus".to_string())),
            },
            SubAgentSource::Other(_) => ThreadSourceColumns {
                source_kind: "subagent",
                subagent_kind: Some("other"),
                subagent_parent_thread_id: None,
                subagent_depth: None,
                subagent_agent_base_name: fallback_agent_base_name.map(str::to_string),
                subagent_agent_title: fallback_agent_title.map(str::to_string),
                subagent_agent_display_name: fallback_agent_display_name.map(str::to_string),
            },
        },
    }
}

fn source_kind_db_value(kind: crate::ThreadSourceKind) -> (&'static str, Option<&'static str>) {
    match kind {
        crate::ThreadSourceKind::Cli => ("cli", None),
        crate::ThreadSourceKind::VsCode => ("vscode", None),
        crate::ThreadSourceKind::Exec => ("exec", None),
        crate::ThreadSourceKind::AppGateway => ("app_gateway", None),
        crate::ThreadSourceKind::SubAgent => ("subagent", None),
        crate::ThreadSourceKind::SubAgentReview => ("subagent", Some("review")),
        crate::ThreadSourceKind::SubAgentCompact => ("subagent", Some("compact")),
        crate::ThreadSourceKind::SubAgentThreadSpawn => ("subagent", Some("thread_spawn")),
        crate::ThreadSourceKind::SubAgentOther => ("subagent", Some("other")),
        crate::ThreadSourceKind::Unknown => ("unknown", None),
    }
}

fn push_thread_source_kind_filter(
    builder: &mut QueryBuilder<'_, Sqlite>,
    source_kinds: &[crate::ThreadSourceKind],
) {
    builder.push(" AND (");
    for (idx, kind) in source_kinds.iter().enumerate() {
        if idx > 0 {
            builder.push(" OR ");
        }
        let (source_kind, subagent_kind) = source_kind_db_value(*kind);
        builder.push("(source_kind = ");
        builder.push_bind(source_kind);
        if let Some(subagent_kind) = subagent_kind {
            builder.push(" AND subagent_kind = ");
            builder.push_bind(subagent_kind);
        }
        builder.push(")");
    }
    builder.push(")");
}

pub(super) fn push_thread_filters<'a>(
    builder: &mut QueryBuilder<'a, Sqlite>,
    archived_only: bool,
    allowed_sources: &'a [String],
    source_kinds: Option<&'a [crate::ThreadSourceKind]>,
    model_providers: Option<&'a [String]>,
    anchor: Option<&crate::Anchor>,
    sort_key: SortKey,
    cwd: Option<&'a str>,
    search_term: Option<&'a str>,
) {
    builder.push(" WHERE 1 = 1");
    if archived_only {
        builder.push(" AND archived = 1");
    } else {
        builder.push(" AND archived = 0");
    }
    builder.push(" AND first_user_message <> ''");
    if let Some(source_kinds) = source_kinds
        && !source_kinds.is_empty()
    {
        push_thread_source_kind_filter(builder, source_kinds);
    } else if !allowed_sources.is_empty() {
        builder.push(" AND source IN (");
        let mut separated = builder.separated(", ");
        for source in allowed_sources {
            separated.push_bind(source);
        }
        separated.push_unseparated(")");
    }
    if let Some(model_providers) = model_providers
        && !model_providers.is_empty()
    {
        builder.push(" AND model_provider IN (");
        let mut separated = builder.separated(", ");
        for provider in model_providers {
            separated.push_bind(provider);
        }
        separated.push_unseparated(")");
    }
    if let Some(cwd) = cwd {
        builder.push(" AND (cwd = ");
        builder.push_bind(cwd);
        let cwd_root = cwd.trim_end_matches(['/', '\\']);
        if !cwd_root.is_empty() {
            let escaped = sqlite_like_escape(cwd_root);
            builder.push(" OR cwd LIKE ");
            builder.push_bind(format!("{escaped}/%"));
            builder.push(" ESCAPE '\\' OR cwd LIKE ");
            builder.push_bind(format!("{escaped}\\\\%"));
            builder.push(" ESCAPE '\\'");
        }
        builder.push(")");
    }
    if let Some(search_term) = search_term {
        let Some(search_query) = thread_search_fts_query(search_term) else {
            builder.push(" AND 0 = 1");
            return;
        };
        builder
            .push(" AND (threads.rowid IN (SELECT rowid FROM threads_fts WHERE threads_fts MATCH ");
        builder.push_bind(search_query.clone());
        builder.push(") OR threads.id IN (SELECT thread_id FROM thread_names WHERE rowid IN (SELECT rowid FROM thread_names_fts WHERE thread_names_fts MATCH ");
        builder.push_bind(search_query);
        builder.push(")))");
    }
    if let Some(anchor) = anchor {
        let anchor_ts = datetime_to_epoch_seconds(anchor.ts);
        let column = match sort_key {
            SortKey::CreatedAt => "created_at",
            SortKey::UpdatedAt => "updated_at",
        };
        builder.push(" AND (");
        builder.push(column);
        builder.push(" < ");
        builder.push_bind(anchor_ts);
        builder.push(" OR (");
        builder.push(column);
        builder.push(" = ");
        builder.push_bind(anchor_ts);
        builder.push(" AND id < ");
        builder.push_bind(anchor.id.to_string());
        builder.push("))");
    }
}

fn sqlite_like_escape(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for ch in value.chars() {
        if matches!(ch, '%' | '_' | '\\') {
            escaped.push('\\');
        }
        escaped.push(ch);
    }
    escaped
}

pub(super) fn push_thread_order_and_limit(
    builder: &mut QueryBuilder<'_, Sqlite>,
    sort_key: SortKey,
    limit: usize,
) {
    let order_column = match sort_key {
        SortKey::CreatedAt => "created_at",
        SortKey::UpdatedAt => "updated_at",
    };
    builder.push(" ORDER BY ");
    builder.push(order_column);
    builder.push(" DESC, id DESC");
    builder.push(" LIMIT ");
    builder.push_bind(limit as i64);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DirectionalThreadSpawnEdgeStatus;
    use crate::runtime::test_support::test_thread_metadata;
    use crate::runtime::test_support::unique_temp_dir;
    use praxis_protocol::protocol::EventMsg;
    use praxis_protocol::protocol::GitInfo;
    use praxis_protocol::protocol::SessionMeta;
    use praxis_protocol::protocol::SessionMetaLine;
    use praxis_protocol::protocol::SessionSource;
    use praxis_protocol::protocol::SubAgentSource;
    use pretty_assertions::assert_eq;
    use std::path::PathBuf;

    #[test]
    fn sqlite_like_escape_escapes_wildcards_and_escape_char() {
        assert_eq!(sqlite_like_escape(r"C:\a%b_c"), r"C:\\a\%b\_c");
    }

    #[test]
    fn thread_search_fts_query_uses_prefix_tokens() {
        assert_eq!(
            thread_search_fts_query("Legacy Codex").as_deref(),
            Some("legacy* AND codex*")
        );
        assert_eq!(thread_search_fts_query("///"), None);
    }

    #[tokio::test]
    async fn list_threads_cwd_filter_matches_descendant_paths() {
        let praxis_home = unique_temp_dir();
        let runtime = StateRuntime::init(praxis_home.clone(), "test-provider".to_string())
            .await
            .expect("state db should initialize");
        let root_id =
            ThreadId::from_string("00000000-0000-0000-0000-000000000201").expect("root id");
        let child_id =
            ThreadId::from_string("00000000-0000-0000-0000-000000000202").expect("child id");
        let sibling_id =
            ThreadId::from_string("00000000-0000-0000-0000-000000000203").expect("sibling id");
        let project_root = praxis_home.join("project");

        runtime
            .upsert_thread(&test_thread_metadata(
                &praxis_home,
                root_id,
                project_root.clone(),
            ))
            .await
            .expect("root thread");
        runtime
            .upsert_thread(&test_thread_metadata(
                &praxis_home,
                child_id,
                project_root.join("src"),
            ))
            .await
            .expect("child thread");
        runtime
            .upsert_thread(&test_thread_metadata(
                &praxis_home,
                sibling_id,
                praxis_home.join("project-sibling"),
            ))
            .await
            .expect("sibling thread");

        let cwd = project_root.display().to_string();
        let allowed_sources = vec!["cli".to_string()];
        let page = runtime
            .list_threads(
                10,
                /*anchor*/ None,
                SortKey::UpdatedAt,
                allowed_sources.as_slice(),
                /*source_kinds*/ None,
                /*model_providers*/ None,
                /*archived_only*/ false,
                Some(cwd.as_str()),
                /*search_term*/ None,
            )
            .await
            .expect("list threads");

        assert_eq!(page.items.len(), 2);
        assert!(page.items.iter().any(|item| item.id == root_id));
        assert!(page.items.iter().any(|item| item.id == child_id));
        assert!(!page.items.iter().any(|item| item.id == sibling_id));
    }

    #[tokio::test]
    async fn list_threads_filters_mixed_source_kinds() {
        let praxis_home = unique_temp_dir();
        let runtime = StateRuntime::init(praxis_home.clone(), "test-provider".to_string())
            .await
            .expect("state db should initialize");
        let cli_id =
            ThreadId::from_string("00000000-0000-0000-0000-000000000921").expect("valid thread id");
        let spawn_id =
            ThreadId::from_string("00000000-0000-0000-0000-000000000922").expect("valid thread id");
        let exec_id =
            ThreadId::from_string("00000000-0000-0000-0000-000000000923").expect("valid thread id");

        runtime
            .upsert_thread(&test_thread_metadata(
                &praxis_home,
                cli_id,
                praxis_home.clone(),
            ))
            .await
            .expect("cli thread");

        let mut spawn = test_thread_metadata(&praxis_home, spawn_id, praxis_home.clone());
        spawn.source =
            crate::extract::enum_to_string(&SessionSource::SubAgent(SubAgentSource::ThreadSpawn {
                parent_thread_id: cli_id,
                depth: 1,
                agent_path: None,
                agent_base_name: None,
                agent_title: None,
                agent_display_name: Some("builder".to_string()),
                agent_role: None,
            }));
        runtime.upsert_thread(&spawn).await.expect("spawn thread");

        let mut exec = test_thread_metadata(&praxis_home, exec_id, praxis_home.clone());
        exec.source = crate::extract::enum_to_string(&SessionSource::Exec);
        runtime.upsert_thread(&exec).await.expect("exec thread");

        let source_kinds = [
            crate::ThreadSourceKind::Cli,
            crate::ThreadSourceKind::SubAgentThreadSpawn,
        ];
        let page = runtime
            .list_threads(
                10,
                /*anchor*/ None,
                SortKey::UpdatedAt,
                &[],
                Some(source_kinds.as_slice()),
                /*model_providers*/ None,
                /*archived_only*/ false,
                /*cwd*/ None,
                /*search_term*/ None,
            )
            .await
            .expect("list threads");

        assert!(page.items.iter().any(|item| item.id == cli_id));
        assert!(page.items.iter().any(|item| item.id == spawn_id));
        assert!(!page.items.iter().any(|item| item.id == exec_id));
    }

    #[tokio::test]
    async fn upsert_thread_keeps_creation_memory_mode_for_existing_rows() {
        let praxis_home = unique_temp_dir();
        let runtime = StateRuntime::init(praxis_home.clone(), "test-provider".to_string())
            .await
            .expect("state db should initialize");
        let thread_id =
            ThreadId::from_string("00000000-0000-0000-0000-000000000123").expect("valid thread id");
        let mut metadata = test_thread_metadata(&praxis_home, thread_id, praxis_home.clone());

        runtime
            .upsert_thread_with_creation_memory_mode(&metadata, Some("disabled"))
            .await
            .expect("initial insert should succeed");

        let memory_mode: String =
            sqlx::query_scalar("SELECT memory_mode FROM threads WHERE id = ?")
                .bind(thread_id.to_string())
                .fetch_one(runtime.pool.as_ref())
                .await
                .expect("memory mode should be readable");
        assert_eq!(memory_mode, "disabled");

        metadata.title = "updated title".to_string();
        runtime
            .upsert_thread(&metadata)
            .await
            .expect("upsert should succeed");

        let memory_mode: String =
            sqlx::query_scalar("SELECT memory_mode FROM threads WHERE id = ?")
                .bind(thread_id.to_string())
                .fetch_one(runtime.pool.as_ref())
                .await
                .expect("memory mode should remain readable");
        assert_eq!(memory_mode, "disabled");
    }

    #[tokio::test]
    async fn apply_rollout_items_restores_memory_mode_from_session_meta() {
        let praxis_home = unique_temp_dir();
        let runtime = StateRuntime::init(praxis_home.clone(), "test-provider".to_string())
            .await
            .expect("state db should initialize");
        let thread_id =
            ThreadId::from_string("00000000-0000-0000-0000-000000000456").expect("valid thread id");
        let metadata = test_thread_metadata(&praxis_home, thread_id, praxis_home.clone());

        runtime
            .upsert_thread(&metadata)
            .await
            .expect("initial upsert should succeed");

        let builder = ThreadMetadataBuilder::new(
            thread_id,
            metadata.rollout_path.clone(),
            metadata.created_at,
            SessionSource::Cli,
        );
        let items = vec![RolloutItem::SessionMeta(SessionMetaLine {
            meta: SessionMeta {
                id: thread_id,
                forked_from_id: None,
                timestamp: metadata.created_at.to_rfc3339(),
                cwd: PathBuf::new(),
                originator: String::new(),
                cli_version: String::new(),
                source: SessionSource::Cli,
                agent_path: None,
                agent_base_name: None,
                agent_title: None,
                agent_display_name: None,
                agent_role: None,
                model_provider: None,
                base_instructions: None,
                dynamic_tools: None,
                memory_mode: Some("polluted".to_string()),
            },
            git: None,
        })];

        runtime
            .apply_rollout_items(
                &builder, &items, /*new_thread_memory_mode*/ None,
                /*updated_at_override*/ None,
            )
            .await
            .expect("apply_rollout_items should succeed");

        let memory_mode = runtime
            .get_thread_memory_mode(thread_id)
            .await
            .expect("memory mode should load");
        assert_eq!(memory_mode.as_deref(), Some("polluted"));
    }

    #[tokio::test]
    async fn apply_rollout_items_preserves_existing_git_branch_and_fills_missing_git_fields() {
        let praxis_home = unique_temp_dir();
        let runtime = StateRuntime::init(praxis_home.clone(), "test-provider".to_string())
            .await
            .expect("state db should initialize");
        let thread_id =
            ThreadId::from_string("00000000-0000-0000-0000-000000000457").expect("valid thread id");
        let mut metadata = test_thread_metadata(&praxis_home, thread_id, praxis_home.clone());
        metadata.git_branch = Some("sqlite-branch".to_string());

        runtime
            .upsert_thread(&metadata)
            .await
            .expect("initial upsert should succeed");

        let created_at = metadata.created_at.to_rfc3339();
        let builder = ThreadMetadataBuilder::new(
            thread_id,
            metadata.rollout_path.clone(),
            metadata.created_at,
            SessionSource::Cli,
        );
        let items = vec![RolloutItem::SessionMeta(SessionMetaLine {
            meta: SessionMeta {
                id: thread_id,
                forked_from_id: None,
                timestamp: created_at,
                cwd: PathBuf::new(),
                originator: String::new(),
                cli_version: String::new(),
                source: SessionSource::Cli,
                agent_path: None,
                agent_base_name: None,
                agent_title: None,
                agent_display_name: None,
                agent_role: None,
                model_provider: None,
                base_instructions: None,
                dynamic_tools: None,
                memory_mode: None,
            },
            git: Some(GitInfo {
                commit_hash: Some(praxis_git_utils::GitSha::new("rollout-sha")),
                branch: Some("rollout-branch".to_string()),
                repository_url: Some("git@example.com:openai/codex.git".to_string()),
            }),
        })];

        runtime
            .apply_rollout_items(
                &builder, &items, /*new_thread_memory_mode*/ None,
                /*updated_at_override*/ None,
            )
            .await
            .expect("apply_rollout_items should succeed");

        let persisted = runtime
            .get_thread(thread_id)
            .await
            .expect("thread should load")
            .expect("thread should exist");
        assert_eq!(persisted.git_sha.as_deref(), Some("rollout-sha"));
        assert_eq!(persisted.git_branch.as_deref(), Some("sqlite-branch"));
        assert_eq!(
            persisted.git_origin_url.as_deref(),
            Some("git@example.com:openai/codex.git")
        );
    }

    #[tokio::test]
    async fn update_thread_git_info_preserves_newer_non_git_metadata() {
        let praxis_home = unique_temp_dir();
        let runtime = StateRuntime::init(praxis_home.clone(), "test-provider".to_string())
            .await
            .expect("state db should initialize");
        let thread_id =
            ThreadId::from_string("00000000-0000-0000-0000-000000000789").expect("valid thread id");
        let metadata = test_thread_metadata(&praxis_home, thread_id, praxis_home.clone());

        runtime
            .upsert_thread(&metadata)
            .await
            .expect("initial upsert should succeed");

        let updated_at = datetime_to_epoch_seconds(
            DateTime::<Utc>::from_timestamp(1_700_000_100, 0).expect("timestamp"),
        );
        sqlx::query(
            "UPDATE threads SET updated_at = ?, tokens_used = ?, first_user_message = ? WHERE id = ?",
        )
        .bind(updated_at)
        .bind(123_i64)
        .bind("newer preview")
        .bind(thread_id.to_string())
        .execute(runtime.pool.as_ref())
        .await
        .expect("concurrent metadata write should succeed");

        let updated = runtime
            .update_thread_git_info(
                thread_id,
                Some(Some("abc123")),
                Some(Some("feature/branch")),
                Some(Some("git@example.com:openai/codex.git")),
            )
            .await
            .expect("git info update should succeed");
        assert!(updated, "git info update should touch the thread row");

        let persisted = runtime
            .get_thread(thread_id)
            .await
            .expect("thread should load")
            .expect("thread should exist");
        assert_eq!(persisted.tokens_used, 123);
        assert_eq!(
            persisted.first_user_message.as_deref(),
            Some("newer preview")
        );
        assert_eq!(datetime_to_epoch_seconds(persisted.updated_at), updated_at);
        assert_eq!(persisted.git_sha.as_deref(), Some("abc123"));
        assert_eq!(persisted.git_branch.as_deref(), Some("feature/branch"));
        assert_eq!(
            persisted.git_origin_url.as_deref(),
            Some("git@example.com:openai/codex.git")
        );
    }

    #[tokio::test]
    async fn insert_thread_if_absent_preserves_existing_metadata() {
        let praxis_home = unique_temp_dir();
        let runtime = StateRuntime::init(praxis_home.clone(), "test-provider".to_string())
            .await
            .expect("state db should initialize");
        let thread_id =
            ThreadId::from_string("00000000-0000-0000-0000-000000000791").expect("valid thread id");

        let mut existing = test_thread_metadata(&praxis_home, thread_id, praxis_home.clone());
        existing.tokens_used = 123;
        existing.first_user_message = Some("newer preview".to_string());
        existing.updated_at = DateTime::<Utc>::from_timestamp(1_700_000_100, 0).expect("timestamp");
        runtime
            .upsert_thread(&existing)
            .await
            .expect("initial upsert should succeed");

        let mut fallback = test_thread_metadata(&praxis_home, thread_id, praxis_home.clone());
        fallback.tokens_used = 0;
        fallback.first_user_message = None;
        fallback.updated_at = DateTime::<Utc>::from_timestamp(1_700_000_000, 0).expect("timestamp");

        let inserted = runtime
            .insert_thread_if_absent(&fallback)
            .await
            .expect("insert should succeed");
        assert!(!inserted, "existing rows should not be overwritten");

        let persisted = runtime
            .get_thread(thread_id)
            .await
            .expect("thread should load")
            .expect("thread should exist");
        assert_eq!(persisted.tokens_used, 123);
        assert_eq!(
            persisted.first_user_message.as_deref(),
            Some("newer preview")
        );
        assert_eq!(
            datetime_to_epoch_seconds(persisted.updated_at),
            datetime_to_epoch_seconds(existing.updated_at)
        );
    }

    #[tokio::test]
    async fn update_thread_git_info_can_clear_fields() {
        let praxis_home = unique_temp_dir();
        let runtime = StateRuntime::init(praxis_home.clone(), "test-provider".to_string())
            .await
            .expect("state db should initialize");
        let thread_id =
            ThreadId::from_string("00000000-0000-0000-0000-000000000790").expect("valid thread id");
        let mut metadata = test_thread_metadata(&praxis_home, thread_id, praxis_home.clone());
        metadata.git_sha = Some("abc123".to_string());
        metadata.git_branch = Some("feature/branch".to_string());
        metadata.git_origin_url = Some("git@example.com:openai/codex.git".to_string());

        runtime
            .upsert_thread(&metadata)
            .await
            .expect("initial upsert should succeed");

        let updated = runtime
            .update_thread_git_info(thread_id, Some(None), Some(None), Some(None))
            .await
            .expect("git info clear should succeed");
        assert!(updated, "git info clear should touch the thread row");

        let persisted = runtime
            .get_thread(thread_id)
            .await
            .expect("thread should load")
            .expect("thread should exist");
        assert_eq!(persisted.git_sha, None);
        assert_eq!(persisted.git_branch, None);
        assert_eq!(persisted.git_origin_url, None);
    }

    #[tokio::test]
    async fn touch_thread_updated_at_updates_only_updated_at() {
        let praxis_home = unique_temp_dir();
        let runtime = StateRuntime::init(praxis_home.clone(), "test-provider".to_string())
            .await
            .expect("state db should initialize");
        let thread_id =
            ThreadId::from_string("00000000-0000-0000-0000-000000000791").expect("valid thread id");
        let mut metadata = test_thread_metadata(&praxis_home, thread_id, praxis_home.clone());
        metadata.title = "original title".to_string();
        metadata.first_user_message = Some("first-user-message".to_string());

        runtime
            .upsert_thread(&metadata)
            .await
            .expect("initial upsert should succeed");

        let touched_at = DateTime::<Utc>::from_timestamp(1_700_001_111, 0).expect("timestamp");
        let touched = runtime
            .touch_thread_updated_at(thread_id, touched_at)
            .await
            .expect("touch should succeed");
        assert!(touched);

        let persisted = runtime
            .get_thread(thread_id)
            .await
            .expect("thread should load")
            .expect("thread should exist");
        assert_eq!(persisted.updated_at, touched_at);
        assert_eq!(persisted.title, "original title");
        assert_eq!(
            persisted.first_user_message.as_deref(),
            Some("first-user-message")
        );
    }

    #[tokio::test]
    async fn apply_rollout_items_uses_override_updated_at_when_provided() {
        let praxis_home = unique_temp_dir();
        let runtime = StateRuntime::init(praxis_home.clone(), "test-provider".to_string())
            .await
            .expect("state db should initialize");
        let thread_id =
            ThreadId::from_string("00000000-0000-0000-0000-000000000792").expect("valid thread id");
        let metadata = test_thread_metadata(&praxis_home, thread_id, praxis_home.clone());

        runtime
            .upsert_thread(&metadata)
            .await
            .expect("initial upsert should succeed");

        let builder = ThreadMetadataBuilder::new(
            thread_id,
            metadata.rollout_path.clone(),
            metadata.created_at,
            SessionSource::Cli,
        );
        let items = vec![RolloutItem::EventMsg(EventMsg::TokenCount(
            praxis_protocol::protocol::TokenCountEvent {
                info: Some(praxis_protocol::protocol::TokenUsageInfo {
                    total_token_usage: praxis_protocol::protocol::TokenUsage {
                        input_tokens: 0,
                        cached_input_tokens: 0,
                        cache_reported_input_tokens: 0,
                        output_tokens: 0,
                        reasoning_output_tokens: 0,
                        total_tokens: 321,
                    },
                    last_token_usage: praxis_protocol::protocol::TokenUsage::default(),
                    model_context_window: None,
                    model_auto_compact_token_limit: None,
                }),
                rate_limits: None,
            },
        ))];
        let override_updated_at =
            DateTime::<Utc>::from_timestamp(1_700_001_234, 0).expect("timestamp");

        runtime
            .apply_rollout_items(
                &builder,
                &items,
                /*new_thread_memory_mode*/ None,
                Some(override_updated_at),
            )
            .await
            .expect("apply_rollout_items should succeed");

        let persisted = runtime
            .get_thread(thread_id)
            .await
            .expect("thread should load")
            .expect("thread should exist");
        assert_eq!(persisted.tokens_used, 321);
        assert_eq!(
            persisted
                .token_usage_info
                .as_ref()
                .map(|info| info.total_token_usage.total_tokens),
            Some(321)
        );
        assert_eq!(persisted.updated_at, override_updated_at);
    }

    #[tokio::test]
    async fn thread_spawn_edges_track_directional_status() {
        let praxis_home = unique_temp_dir();
        let runtime = StateRuntime::init(praxis_home, "test-provider".to_string())
            .await
            .expect("state db should initialize");
        let parent_thread_id =
            ThreadId::from_string("00000000-0000-0000-0000-000000000900").expect("valid thread id");
        let child_thread_id =
            ThreadId::from_string("00000000-0000-0000-0000-000000000901").expect("valid thread id");
        let grandchild_thread_id =
            ThreadId::from_string("00000000-0000-0000-0000-000000000902").expect("valid thread id");

        runtime
            .upsert_thread_spawn_edge(
                parent_thread_id,
                child_thread_id,
                DirectionalThreadSpawnEdgeStatus::Open,
            )
            .await
            .expect("child edge insert should succeed");
        runtime
            .upsert_thread_spawn_edge(
                child_thread_id,
                grandchild_thread_id,
                DirectionalThreadSpawnEdgeStatus::Open,
            )
            .await
            .expect("grandchild edge insert should succeed");

        let children = runtime
            .list_thread_spawn_children_with_status(
                parent_thread_id,
                DirectionalThreadSpawnEdgeStatus::Open,
            )
            .await
            .expect("open child list should load");
        assert_eq!(children, vec![child_thread_id]);

        let descendants = runtime
            .list_thread_spawn_descendants_with_status(
                parent_thread_id,
                DirectionalThreadSpawnEdgeStatus::Open,
            )
            .await
            .expect("open descendants should load");
        assert_eq!(descendants, vec![child_thread_id, grandchild_thread_id]);

        runtime
            .set_thread_spawn_edge_status(child_thread_id, DirectionalThreadSpawnEdgeStatus::Closed)
            .await
            .expect("edge close should succeed");

        let open_children = runtime
            .list_thread_spawn_children_with_status(
                parent_thread_id,
                DirectionalThreadSpawnEdgeStatus::Open,
            )
            .await
            .expect("open child list should load");
        assert_eq!(open_children, Vec::<ThreadId>::new());

        let closed_children = runtime
            .list_thread_spawn_children_with_status(
                parent_thread_id,
                DirectionalThreadSpawnEdgeStatus::Closed,
            )
            .await
            .expect("closed child list should load");
        assert_eq!(closed_children, vec![child_thread_id]);

        let closed_descendants = runtime
            .list_thread_spawn_descendants_with_status(
                parent_thread_id,
                DirectionalThreadSpawnEdgeStatus::Closed,
            )
            .await
            .expect("closed descendants should load");
        assert_eq!(closed_descendants, vec![child_thread_id]);

        let open_descendants_from_child = runtime
            .list_thread_spawn_descendants_with_status(
                child_thread_id,
                DirectionalThreadSpawnEdgeStatus::Open,
            )
            .await
            .expect("open descendants from child should load");
        assert_eq!(open_descendants_from_child, vec![grandchild_thread_id]);
    }
}
