use crate::CapabilityId;
use crate::CapabilityManifest;
use crate::ScopeGraph;
use crate::ScopeId;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::fmt;

#[derive(Debug, Clone)]
pub struct CapabilityGraph {
    scopes: ScopeGraph,
    manifests: BTreeMap<ScopedCapabilityKey, CapabilityManifest>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct ScopedCapabilityKey {
    pub(crate) id: CapabilityId,
    pub(crate) scope: ScopeId,
}

impl CapabilityGraph {
    pub fn new(scopes: ScopeGraph) -> Self {
        Self {
            scopes,
            manifests: BTreeMap::new(),
        }
    }

    pub fn scopes(&self) -> &ScopeGraph {
        &self.scopes
    }

    pub(crate) fn scopes_mut(&mut self) -> &mut ScopeGraph {
        &mut self.scopes
    }

    pub fn manifests(&self) -> impl Iterator<Item = &CapabilityManifest> {
        self.manifests.values()
    }

    pub fn visible(
        &self,
        request_scope: &ScopeId,
        id: &CapabilityId,
    ) -> Option<&CapabilityManifest> {
        self.manifests.values().find(|manifest| {
            &manifest.id == id && self.scopes.can_see(request_scope, &manifest.scope)
        })
    }

    fn any(&self, id: &CapabilityId) -> Option<&CapabilityManifest> {
        self.manifests.values().find(|manifest| &manifest.id == id)
    }

    pub fn insert(&mut self, manifest: CapabilityManifest) -> Result<(), CapabilityGraphError> {
        if !self.scopes.contains(&manifest.scope) {
            return Err(CapabilityGraphError::MissingScope {
                capability: manifest.id,
                scope: manifest.scope,
            });
        }
        if let Some(existing) = self.manifests.values().find(|existing| {
            existing.id == manifest.id && self.scopes.can_coexist(&existing.scope, &manifest.scope)
        }) {
            return Err(CapabilityGraphError::DuplicateCapability {
                id: manifest.id,
                first_owner: existing.owner.clone(),
                second_owner: manifest.owner,
            });
        }
        self.manifests.insert(
            ScopedCapabilityKey {
                id: manifest.id.clone(),
                scope: manifest.scope.clone(),
            },
            manifest,
        );
        Ok(())
    }

    pub fn remove_in_scope(
        &mut self,
        id: &CapabilityId,
        scope: &ScopeId,
    ) -> Option<CapabilityManifest> {
        self.manifests.remove(&ScopedCapabilityKey {
            id: id.clone(),
            scope: scope.clone(),
        })
    }

    pub fn validate(&self) -> Result<(), CapabilityGraphError> {
        for manifest in self.manifests.values() {
            for dependency in &manifest.dependencies {
                let dependency_manifest = match self.visible(&manifest.scope, dependency) {
                    Some(dependency_manifest) => dependency_manifest,
                    None if self.any(dependency).is_some() => {
                        let dependency_scope = self
                            .any(dependency)
                            .map(|dependency| dependency.scope.clone())
                            .unwrap_or_else(|| manifest.scope.clone());
                        return Err(CapabilityGraphError::InvisibleDependency {
                            capability: manifest.id.clone(),
                            dependency: dependency.clone(),
                            capability_scope: manifest.scope.clone(),
                            dependency_scope,
                        });
                    }
                    None => {
                        return Err(CapabilityGraphError::MissingDependency {
                            capability: manifest.id.clone(),
                            dependency: dependency.clone(),
                        });
                    }
                };
                debug_assert!(
                    self.scopes
                        .can_see(&manifest.scope, &dependency_manifest.scope)
                );
            }
            for conflict in &manifest.conflicts {
                if self.manifests.values().any(|other| {
                    &other.id == conflict && self.scopes.can_coexist(&manifest.scope, &other.scope)
                }) {
                    return Err(CapabilityGraphError::Conflict {
                        capability: manifest.id.clone(),
                        conflict: conflict.clone(),
                    });
                }
            }
            self.resolve(&manifest.scope, [manifest.id.clone()])?;
        }
        Ok(())
    }

    pub fn resolve(
        &self,
        request_scope: &ScopeId,
        roots: impl IntoIterator<Item = CapabilityId>,
    ) -> Result<ResolvedCapabilityGraph, CapabilityGraphError> {
        if !self.scopes.contains(request_scope) {
            return Err(CapabilityGraphError::MissingRequestScope {
                scope: request_scope.clone(),
            });
        }

        let mut closure = BTreeSet::new();
        let mut pending: Vec<_> = roots.into_iter().collect();
        pending.sort();
        pending.reverse();
        while let Some(id) = pending.pop() {
            if !closure.insert(id.clone()) {
                continue;
            }
            let manifest = match self.visible(request_scope, &id) {
                Some(manifest) => manifest,
                None if self.any(&id).is_some() => {
                    let capability_scope = self
                        .any(&id)
                        .map(|manifest| manifest.scope.clone())
                        .unwrap_or_else(|| request_scope.clone());
                    return Err(CapabilityGraphError::InvisibleRoot {
                        capability: id,
                        capability_scope,
                        request_scope: request_scope.clone(),
                    });
                }
                None => {
                    return Err(CapabilityGraphError::MissingRoot { capability: id });
                }
            };
            let mut dependencies: Vec<_> = manifest.dependencies.iter().cloned().collect();
            dependencies.reverse();
            pending.extend(dependencies);
        }

        let ordered = self.topological_order(request_scope, closure.clone())?;
        for id in &ordered {
            let manifest = self.visible(request_scope, id).ok_or_else(|| {
                CapabilityGraphError::MissingRoot {
                    capability: id.clone(),
                }
            })?;
            for conflict in &manifest.conflicts {
                if closure.contains(conflict) {
                    return Err(CapabilityGraphError::Conflict {
                        capability: id.clone(),
                        conflict: conflict.clone(),
                    });
                }
            }
        }
        Ok(ResolvedCapabilityGraph {
            request_scope: request_scope.clone(),
            ordered,
        })
    }

    fn topological_order(
        &self,
        request_scope: &ScopeId,
        ids: impl IntoIterator<Item = CapabilityId>,
    ) -> Result<Vec<CapabilityId>, CapabilityGraphError> {
        let included: BTreeSet<_> = ids.into_iter().collect();
        let mut visiting = BTreeSet::new();
        let mut visited = BTreeSet::new();
        let mut path = Vec::new();
        let mut ordered = Vec::new();

        for id in &included {
            self.visit(
                request_scope,
                id,
                &included,
                &mut visiting,
                &mut visited,
                &mut path,
                &mut ordered,
            )?;
        }
        Ok(ordered)
    }

    fn visit(
        &self,
        request_scope: &ScopeId,
        id: &CapabilityId,
        included: &BTreeSet<CapabilityId>,
        visiting: &mut BTreeSet<CapabilityId>,
        visited: &mut BTreeSet<CapabilityId>,
        path: &mut Vec<CapabilityId>,
        ordered: &mut Vec<CapabilityId>,
    ) -> Result<(), CapabilityGraphError> {
        if visited.contains(id) {
            return Ok(());
        }
        if visiting.contains(id) {
            let cycle_start = path
                .iter()
                .position(|candidate| candidate == id)
                .unwrap_or(0);
            let mut cycle = path[cycle_start..].to_vec();
            cycle.push(id.clone());
            return Err(CapabilityGraphError::DependencyCycle { cycle });
        }

        let manifest =
            self.visible(request_scope, id)
                .ok_or_else(|| CapabilityGraphError::MissingRoot {
                    capability: id.clone(),
                })?;
        visiting.insert(id.clone());
        path.push(id.clone());
        for dependency in &manifest.dependencies {
            self.visible(&manifest.scope, dependency).ok_or_else(|| {
                CapabilityGraphError::MissingDependency {
                    capability: id.clone(),
                    dependency: dependency.clone(),
                }
            })?;
            if included.contains(dependency) {
                self.visit(
                    request_scope,
                    dependency,
                    included,
                    visiting,
                    visited,
                    path,
                    ordered,
                )?;
            }
        }
        path.pop();
        visiting.remove(id);
        visited.insert(id.clone());
        ordered.push(id.clone());
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedCapabilityGraph {
    request_scope: ScopeId,
    ordered: Vec<CapabilityId>,
}

impl ResolvedCapabilityGraph {
    pub fn request_scope(&self) -> &ScopeId {
        &self.request_scope
    }

    pub fn ordered_ids(&self) -> &[CapabilityId] {
        &self.ordered
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CapabilityGraphError {
    MissingScope {
        capability: CapabilityId,
        scope: ScopeId,
    },
    MissingRequestScope {
        scope: ScopeId,
    },
    DuplicateCapability {
        id: CapabilityId,
        first_owner: crate::CapabilityOwnerId,
        second_owner: crate::CapabilityOwnerId,
    },
    MissingRoot {
        capability: CapabilityId,
    },
    MissingDependency {
        capability: CapabilityId,
        dependency: CapabilityId,
    },
    InvisibleRoot {
        capability: CapabilityId,
        capability_scope: ScopeId,
        request_scope: ScopeId,
    },
    InvisibleDependency {
        capability: CapabilityId,
        capability_scope: ScopeId,
        dependency: CapabilityId,
        dependency_scope: ScopeId,
    },
    DependencyCycle {
        cycle: Vec<CapabilityId>,
    },
    Conflict {
        capability: CapabilityId,
        conflict: CapabilityId,
    },
}

impl fmt::Display for CapabilityGraphError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for CapabilityGraphError {}
