use super::*;

pub(super) fn thread_search_fts_query(search_term: &str) -> Option<String> {
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

pub(crate) fn push_thread_filters<'a>(
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

pub(super) fn sqlite_like_escape(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for ch in value.chars() {
        if matches!(ch, '%' | '_' | '\\') {
            escaped.push('\\');
        }
        escaped.push(ch);
    }
    escaped
}

pub(crate) fn push_thread_order_and_limit(
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
