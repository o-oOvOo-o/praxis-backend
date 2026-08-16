use std::collections::HashMap;
use std::ops::Deref;
use std::sync::Arc;

use anyhow::Context;
use praxis_capability_runtime::CapabilityId;
use praxis_capability_runtime::CapabilityKind;
use praxis_capability_runtime::CapabilityManifest;
use praxis_capability_runtime::CapabilityOwnerId;
use praxis_capability_runtime::CapabilityRuntime;
use praxis_capability_runtime::CapabilityScope;
use praxis_capability_runtime::ScopeId;
use praxis_capability_runtime::ScopeKind;
use praxis_capability_runtime::TypedCapability;
use praxis_mcp::mcp_connection_manager::McpConnectionManager;
use praxis_mcp::mcp_connection_manager::ToolInfo;
use praxis_protocol::ThreadId;
use tokio_util::sync::CancellationToken;

use crate::mcp::McpManager;

const MCP_DISCOVERY_ID: &str = "praxis.core.mcp.discovery";
const MCP_CONNECTIONS_ID: &str = "praxis.core.mcp.connections";
const MCP_SNAPSHOT_ID: &str = "praxis.core.mcp.resolved";
const MCP_OWNER_ID: &str = "praxis.core.mcp";

pub type McpDiscoveryCapability = TypedCapability<Arc<McpManager>>;

struct McpConnectionGeneration {
    manager: Arc<McpConnectionManager>,
}

#[derive(Clone)]
pub(crate) struct McpConnectionCapability {
    generation: TypedCapability<McpConnectionGeneration>,
}

impl McpConnectionCapability {
    pub(crate) fn manager(&self) -> &McpConnectionManager {
        self.generation.value().manager.as_ref()
    }

    #[cfg(test)]
    pub(crate) fn lease(&self) -> &praxis_capability_runtime::CapabilityLease {
        self.generation.lease()
    }
}

impl Deref for McpConnectionCapability {
    type Target = McpConnectionManager;

    fn deref(&self) -> &Self::Target {
        self.manager()
    }
}

struct ResolvedMcpSnapshot {
    connections: McpConnectionCapability,
    has_servers: bool,
    tools: HashMap<String, ToolInfo>,
}

#[derive(Clone)]
pub(crate) struct ResolvedMcpSnapshotCapability {
    snapshot: TypedCapability<ResolvedMcpSnapshot>,
    _scope: Arc<CapabilityScope>,
}

impl ResolvedMcpSnapshotCapability {
    pub(crate) fn connections(&self) -> &McpConnectionCapability {
        &self.snapshot.value().connections
    }

    pub(crate) fn has_servers(&self) -> bool {
        self.snapshot.value().has_servers
    }

    pub(crate) fn tools(&self) -> &HashMap<String, ToolInfo> {
        &self.snapshot.value().tools
    }

    #[cfg(test)]
    pub(crate) fn lease(&self) -> &praxis_capability_runtime::CapabilityLease {
        self.snapshot.lease()
    }
}

pub(crate) fn publish_mcp_discovery(
    runtime: &CapabilityRuntime,
    manager: Arc<McpManager>,
) -> anyhow::Result<McpDiscoveryCapability> {
    let capability_id = CapabilityId::new(MCP_DISCOVERY_ID)?;
    let owner_id = CapabilityOwnerId::new(MCP_OWNER_ID)?;
    let mut transaction = runtime.begin_transaction(owner_id.clone(), ScopeId::process());
    transaction.stage_typed(
        CapabilityManifest::new(
            capability_id.clone(),
            CapabilityKind::McpServer,
            owner_id,
            ScopeId::process(),
        ),
        move || Ok((manager, Box::new(|| Ok(())))),
    )?;
    transaction.commit()?;
    runtime
        .acquire_typed(&ScopeId::process(), &capability_id)?
        .context("MCP discovery capability was not active after a successful commit")
}

pub(crate) fn publish_mcp_connections(
    thread_scope: &CapabilityScope,
    manager: McpConnectionManager,
    cancellation: CancellationToken,
) -> anyhow::Result<McpConnectionCapability> {
    let capability_id = CapabilityId::new(MCP_CONNECTIONS_ID)?;
    let owner_id = CapabilityOwnerId::new(MCP_OWNER_ID)?;
    let mut transaction = thread_scope.begin_transaction(owner_id.clone());
    transaction.stage_typed(
        CapabilityManifest::new(
            capability_id.clone(),
            CapabilityKind::McpServer,
            owner_id,
            thread_scope.id().clone(),
        )
        .with_dependencies([CapabilityId::new(MCP_DISCOVERY_ID)?]),
        move || {
            let disposal_cancellation = cancellation.clone();
            Ok((
                McpConnectionGeneration {
                    manager: Arc::new(manager),
                },
                Box::new(move || {
                    disposal_cancellation.cancel();
                    Ok(())
                }),
            ))
        },
    )?;
    transaction.commit()?;
    let generation = thread_scope
        .acquire_typed(&capability_id)?
        .context("MCP connections capability was not active after a successful commit")?;
    Ok(McpConnectionCapability { generation })
}

pub(crate) async fn publish_resolved_mcp_snapshot(
    thread_scope: &CapabilityScope,
    conversation_id: ThreadId,
    turn_id: &str,
    connections: McpConnectionCapability,
) -> anyhow::Result<ResolvedMcpSnapshotCapability> {
    let has_servers = connections.has_servers();
    let tools = connections.list_all_tools().await;
    publish_resolved_mcp_snapshot_with_tools(
        thread_scope,
        conversation_id,
        turn_id,
        connections,
        has_servers,
        tools,
    )
}

fn publish_resolved_mcp_snapshot_with_tools(
    thread_scope: &CapabilityScope,
    conversation_id: ThreadId,
    turn_id: &str,
    connections: McpConnectionCapability,
    has_servers: bool,
    tools: HashMap<String, ToolInfo>,
) -> anyhow::Result<ResolvedMcpSnapshotCapability> {
    let scope = Arc::new(thread_scope.runtime().open_child_scope(
        ScopeId::new(format!("turn:{conversation_id}:{turn_id}"))?,
        ScopeKind::Turn,
        thread_scope.id().clone(),
    )?);
    let capability_id = CapabilityId::new(MCP_SNAPSHOT_ID)?;
    let owner_id = CapabilityOwnerId::new(MCP_OWNER_ID)?;
    let mut transaction = scope.begin_transaction(owner_id.clone());
    transaction.stage_typed(
        CapabilityManifest::new(
            capability_id.clone(),
            CapabilityKind::McpServer,
            owner_id,
            scope.id().clone(),
        )
        .with_dependencies([CapabilityId::new(MCP_CONNECTIONS_ID)?]),
        move || {
            Ok((
                ResolvedMcpSnapshot {
                    connections,
                    has_servers,
                    tools,
                },
                Box::new(|| Ok(())),
            ))
        },
    )?;
    transaction.commit()?;
    let snapshot = scope
        .acquire_typed(&capability_id)?
        .context("Resolved MCP capability was not active after a successful commit")?;
    Ok(ResolvedMcpSnapshotCapability {
        snapshot,
        _scope: scope,
    })
}

#[cfg(test)]
mod tests {
    use praxis_capability_runtime::CapabilityLifecycle;
    use praxis_protocol::protocol::AskForApproval;
    use praxis_config::Constrained;

    fn uninitialized() -> McpConnectionManager {
        McpConnectionManager::new_uninitialized(&Constrained::allow_any(AskForApproval::Never))
    }

    #[tokio::test]
    async fn mcp_lifecycle_is_process_discovery_thread_connections_and_turn_snapshot() {
        let runtime = super::super::new_runtime();
        let plugins = Arc::new(crate::plugins::PluginsManager::new(
            std::path::PathBuf::from("test"),
        ));
        let discovery = super::publish_mcp_discovery(
            &runtime,
            Arc::new(crate::mcp::McpManager::new(plugins)),
        )
        .expect("publish MCP discovery");
        let thread_id = ThreadId::default();
        let thread = super::super::open_thread_scope(&runtime, thread_id)
            .expect("open MCP test thread");
        let connections = super::publish_mcp_connections(
            &thread,
            uninitialized(),
            CancellationToken::new(),
        )
        .expect("publish MCP connections");
        let snapshot = super::publish_resolved_mcp_snapshot(
            &thread,
            thread_id,
            "turn",
            connections.clone(),
        )
        .await
        .expect("publish resolved MCP snapshot");

        assert_eq!(discovery.lease().capability_id().as_str(), super::MCP_DISCOVERY_ID);
        assert_eq!(connections.lease().capability_id().as_str(), super::MCP_CONNECTIONS_ID);
        assert_eq!(snapshot.lease().capability_id().as_str(), super::MCP_SNAPSHOT_ID);
        assert_eq!(
            snapshot.connections().lease().generation_id(),
            connections.lease().generation_id()
        );
    }

    #[tokio::test]
    async fn replaced_mcp_connections_cancel_only_after_snapshot_leases_drain() {
        let runtime = super::super::new_runtime();
        let plugins = Arc::new(crate::plugins::PluginsManager::new(
            std::path::PathBuf::from("test"),
        ));
        let _discovery = super::publish_mcp_discovery(
            &runtime,
            Arc::new(crate::mcp::McpManager::new(plugins)),
        )
        .expect("publish MCP discovery");
        let thread_id = ThreadId::default();
        let thread = super::super::open_thread_scope(&runtime, thread_id)
            .expect("open MCP test thread");
        let old_cancellation = CancellationToken::new();
        let first = super::publish_mcp_connections(
            &thread,
            uninitialized(),
            old_cancellation.clone(),
        )
        .expect("publish first MCP connections");
        let snapshot = super::publish_resolved_mcp_snapshot(
            &thread,
            thread_id,
            "turn",
            first.clone(),
        )
        .await
        .expect("publish resolved MCP snapshot");
        let old_generation = first.lease().generation_id();

        let _second = super::publish_mcp_connections(
            &thread,
            uninitialized(),
            CancellationToken::new(),
        )
        .expect("publish replacement MCP connections");

        assert_eq!(first.lease().lifecycle(), CapabilityLifecycle::Quiescing);
        drop(first);
        assert!(!old_cancellation.is_cancelled());
        drop(snapshot);
        assert!(old_cancellation.is_cancelled());
        assert_eq!(
            runtime
                .snapshot()
                .generations
                .iter()
                .find(|generation| generation.id == old_generation)
                .expect("old MCP generation remains observable")
                .lifecycle,
            CapabilityLifecycle::Disposed
        );
    }
}
