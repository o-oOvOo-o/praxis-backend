use super::ExternalAgentMigrationItem;
use super::ExternalAgentMigrationItemType;
use std::path::PathBuf;

const EXTERNAL_AGENT_MIGRATION_DETECT_METRIC: &str = "praxis.external_agent_migration.detect";
const EXTERNAL_AGENT_MIGRATION_IMPORT_METRIC: &str = "praxis.external_agent_migration.import";

#[derive(Default)]
pub(super) struct ExternalAgentMigrationLedger {
    entries: Vec<ExternalAgentMigrationItem>,
}

impl ExternalAgentMigrationLedger {
    pub(super) fn push_detected(
        &mut self,
        cwd: Option<PathBuf>,
        item_type: ExternalAgentMigrationItemType,
        description: String,
        skills_count: Option<usize>,
    ) {
        emit_migration_metric(
            EXTERNAL_AGENT_MIGRATION_DETECT_METRIC,
            item_type,
            skills_count,
        );
        self.entries.push(ExternalAgentMigrationItem {
            item_type,
            description,
            cwd,
        });
    }

    pub(super) fn into_items(self) -> Vec<ExternalAgentMigrationItem> {
        self.entries
    }
}

pub(super) fn emit_import_metric(
    item_type: ExternalAgentMigrationItemType,
    skills_count: Option<usize>,
) {
    emit_migration_metric(
        EXTERNAL_AGENT_MIGRATION_IMPORT_METRIC,
        item_type,
        skills_count,
    );
}

pub(super) fn migration_metric_tags(
    item_type: ExternalAgentMigrationItemType,
    skills_count: Option<usize>,
) -> Vec<(&'static str, String)> {
    let migration_type = match item_type {
        ExternalAgentMigrationItemType::Config => "config",
        ExternalAgentMigrationItemType::Skills => "skills",
        ExternalAgentMigrationItemType::AgentsMd => "agents_md",
    };
    let mut tags = vec![("migration_type", migration_type.to_string())];
    if item_type == ExternalAgentMigrationItemType::Skills {
        tags.push(("skills_count", skills_count.unwrap_or(0).to_string()));
    }
    tags
}

fn emit_migration_metric(
    metric_name: &str,
    item_type: ExternalAgentMigrationItemType,
    skills_count: Option<usize>,
) {
    let Some(metrics) = praxis_otel::metrics::global() else {
        return;
    };
    let tags = migration_metric_tags(item_type, skills_count);
    let tag_refs = tags
        .iter()
        .map(|(key, value)| (*key, value.as_str()))
        .collect::<Vec<_>>();
    let _ = metrics.counter(metric_name, /*inc*/ 1, &tag_refs);
}
