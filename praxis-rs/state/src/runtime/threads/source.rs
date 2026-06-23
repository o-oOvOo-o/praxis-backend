use super::*;
use praxis_protocol::protocol::SessionSource;
use praxis_protocol::protocol::SubAgentSource;

#[derive(Debug, Clone)]
pub(super) struct ThreadSourceColumns {
    pub(super) source_kind: &'static str,
    pub(super) subagent_kind: Option<&'static str>,
    pub(super) subagent_parent_thread_id: Option<String>,
    pub(super) subagent_depth: Option<i64>,
    pub(super) subagent_agent_base_name: Option<String>,
    pub(super) subagent_agent_title: Option<String>,
    pub(super) subagent_agent_display_name: Option<String>,
}

pub(super) fn thread_spawn_parent_thread_id_from_source_str(source: &str) -> Option<ThreadId> {
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

pub(super) fn thread_source_columns_from_source_str(
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
