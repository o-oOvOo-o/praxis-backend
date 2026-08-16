use crate::ScopeId;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ScopeKind {
    Process,
    Workspace,
    Thread,
    Turn,
    ToolCall,
    Game,
    World,
    Level,
    Entity,
    Conversation,
    Custom,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ScopeNode {
    kind: ScopeKind,
    parent: Option<ScopeId>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ScopeGraph {
    nodes: BTreeMap<ScopeId, ScopeNode>,
}

impl ScopeGraph {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn single_root(id: ScopeId, kind: ScopeKind) -> Self {
        let mut nodes = BTreeMap::new();
        nodes.insert(id, ScopeNode { kind, parent: None });
        Self { nodes }
    }

    pub fn add_root(&mut self, id: ScopeId, kind: ScopeKind) -> Result<(), ScopeGraphError> {
        self.insert(id, kind, None)
    }

    pub fn add_child(
        &mut self,
        id: ScopeId,
        kind: ScopeKind,
        parent: ScopeId,
    ) -> Result<(), ScopeGraphError> {
        if !self.nodes.contains_key(&parent) {
            return Err(ScopeGraphError::MissingParent { id, parent });
        }
        self.insert(id, kind, Some(parent))
    }

    pub fn ensure_child(
        &mut self,
        id: ScopeId,
        kind: ScopeKind,
        parent: ScopeId,
    ) -> Result<(), ScopeGraphError> {
        if let Some(existing) = self.nodes.get(&id) {
            if existing.kind == kind && existing.parent.as_ref() == Some(&parent) {
                return Ok(());
            }
            return Err(ScopeGraphError::ConflictingScope {
                id,
                existing_kind: existing.kind,
                existing_parent: existing.parent.clone(),
                requested_kind: kind,
                requested_parent: Some(parent),
            });
        }
        if !self.nodes.contains_key(&parent) {
            return Err(ScopeGraphError::MissingParent { id, parent });
        }
        self.insert(id, kind, Some(parent))
    }

    pub fn contains(&self, id: &ScopeId) -> bool {
        self.nodes.contains_key(id)
    }

    pub fn kind(&self, id: &ScopeId) -> Option<ScopeKind> {
        self.nodes.get(id).map(|node| node.kind)
    }

    pub fn can_see(&self, request_scope: &ScopeId, contribution_scope: &ScopeId) -> bool {
        let mut current = Some(request_scope);
        let mut visited = BTreeSet::new();
        while let Some(scope) = current {
            if scope == contribution_scope {
                return true;
            }
            if !visited.insert(scope.clone()) {
                return false;
            }
            current = self.nodes.get(scope).and_then(|node| node.parent.as_ref());
        }
        false
    }

    pub fn can_coexist(&self, first: &ScopeId, second: &ScopeId) -> bool {
        self.can_see(first, second) || self.can_see(second, first)
    }

    fn insert(
        &mut self,
        id: ScopeId,
        kind: ScopeKind,
        parent: Option<ScopeId>,
    ) -> Result<(), ScopeGraphError> {
        if self.nodes.contains_key(&id) {
            return Err(ScopeGraphError::DuplicateScope { id });
        }
        self.nodes.insert(id, ScopeNode { kind, parent });
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScopeGraphError {
    DuplicateScope {
        id: ScopeId,
    },
    MissingParent {
        id: ScopeId,
        parent: ScopeId,
    },
    ConflictingScope {
        id: ScopeId,
        existing_kind: ScopeKind,
        existing_parent: Option<ScopeId>,
        requested_kind: ScopeKind,
        requested_parent: Option<ScopeId>,
    },
}

impl fmt::Display for ScopeGraphError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateScope { id } => write!(formatter, "scope {id} is already registered"),
            Self::MissingParent { id, parent } => {
                write!(formatter, "scope {id} references missing parent {parent}")
            }
            Self::ConflictingScope {
                id,
                existing_kind,
                existing_parent,
                requested_kind,
                requested_parent,
            } => write!(
                formatter,
                "scope {id} already exists as {existing_kind:?} under {existing_parent:?}, \
                 requested {requested_kind:?} under {requested_parent:?}"
            ),
        }
    }
}

impl std::error::Error for ScopeGraphError {}
