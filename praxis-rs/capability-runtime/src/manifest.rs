use crate::CapabilityId;
use crate::CapabilityOwnerId;
use crate::ScopeId;
use std::collections::BTreeSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CapabilityKind {
    Service,
    Tool,
    Provider,
    Hook,
    Skill,
    McpServer,
    App,
    EditorAction,
    EditorTab,
    KeyBinding,
    ContextMenu,
    GameSystem,
    GameResource,
    GameEvent,
    GameAction,
    Presentation,
    Custom,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilityManifest {
    pub id: CapabilityId,
    pub kind: CapabilityKind,
    pub owner: CapabilityOwnerId,
    pub scope: ScopeId,
    pub dependencies: BTreeSet<CapabilityId>,
    pub conflicts: BTreeSet<CapabilityId>,
}

impl CapabilityManifest {
    pub fn new(
        id: CapabilityId,
        kind: CapabilityKind,
        owner: CapabilityOwnerId,
        scope: ScopeId,
    ) -> Self {
        Self {
            id,
            kind,
            owner,
            scope,
            dependencies: BTreeSet::new(),
            conflicts: BTreeSet::new(),
        }
    }

    pub fn with_dependencies(
        mut self,
        dependencies: impl IntoIterator<Item = CapabilityId>,
    ) -> Self {
        self.dependencies.extend(dependencies);
        self
    }

    pub fn with_conflicts(mut self, conflicts: impl IntoIterator<Item = CapabilityId>) -> Self {
        self.conflicts.extend(conflicts);
        self
    }
}
