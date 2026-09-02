use crate::CapabilityGraph;
use crate::CapabilityGraphError;
use crate::CapabilityId;
use crate::CapabilityKind;
use crate::CapabilityLease;
use crate::CapabilityManifest;
use crate::CapabilityOwnerId;
use crate::GenerationId;
use crate::ScopeGraph;
use crate::ScopeGraphError;
use crate::ScopeId;
use crate::ScopeKind;
use crate::TypedCapability;
use crate::graph::ScopedCapabilityKey;
use std::any::Any;
use std::any::type_name;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::fmt;
use std::panic::AssertUnwindSafe;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::MutexGuard;
use std::sync::Weak;
use std::sync::mpsc;
use std::thread;

pub type CapabilityDisposer = Box<dyn FnOnce() -> Result<(), String> + Send + 'static>;
pub type CapabilityActivation =
    Box<dyn FnOnce() -> Result<CapabilityDisposer, String> + Send + 'static>;

const DISPOSAL_REAPER_STACK_BYTES: usize = 512 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CapabilityLifecycle {
    Discovered,
    Resolved,
    Staged,
    Validated,
    Active,
    Quiescing,
    Retired,
    Disposed,
}

struct StagedCapability {
    manifest: CapabilityManifest,
    activate: Box<dyn FnOnce() -> Result<ActivatedCapability, String> + Send + 'static>,
}

struct ActivatedCapability {
    payload: Arc<dyn Any + Send + Sync>,
    disposer: CapabilityDisposer,
}

struct GenerationRecord {
    owner: CapabilityOwnerId,
    scope: ScopeId,
    lifecycle: CapabilityLifecycle,
    leases: usize,
    capabilities: Vec<CapabilityId>,
    payloads: BTreeMap<CapabilityId, Arc<dyn Any + Send + Sync>>,
    disposers: Vec<CapabilityDisposer>,
    disposal_failures: Vec<String>,
}

#[derive(Clone)]
pub struct CapabilityRuntime {
    pub(crate) inner: Arc<RuntimeInner>,
}

pub(crate) struct RuntimeInner {
    commit_gate: Mutex<()>,
    state: Mutex<RuntimeState>,
    disposal_tx: mpsc::Sender<DisposalJob>,
}

struct DisposalJob {
    generation: GenerationId,
    disposers: Vec<CapabilityDisposer>,
}

struct RuntimeState {
    next_generation: u64,
    graph: CapabilityGraph,
    scope_handles: BTreeMap<ScopeId, usize>,
    active_generations: BTreeMap<ScopedCapabilityKey, GenerationId>,
    generations: BTreeMap<GenerationId, GenerationRecord>,
}

impl CapabilityRuntime {
    pub fn new(scopes: ScopeGraph) -> Self {
        let (disposal_tx, disposal_rx) = mpsc::channel();
        let inner = Arc::new(RuntimeInner {
            commit_gate: Mutex::new(()),
            state: Mutex::new(RuntimeState {
                next_generation: 1,
                graph: CapabilityGraph::new(scopes),
                scope_handles: BTreeMap::new(),
                active_generations: BTreeMap::new(),
                generations: BTreeMap::new(),
            }),
            disposal_tx,
        });
        spawn_disposal_reaper(Arc::downgrade(&inner), disposal_rx);
        Self { inner }
    }

    pub fn begin_transaction(
        &self,
        owner: CapabilityOwnerId,
        scope: ScopeId,
    ) -> CapabilityTransaction {
        CapabilityTransaction {
            runtime: self.clone(),
            owner,
            scope,
            staged: BTreeMap::new(),
        }
    }

    pub fn ensure_child_scope(
        &self,
        id: ScopeId,
        kind: ScopeKind,
        parent: ScopeId,
    ) -> Result<(), ScopeGraphError> {
        let _commit_guard = self.inner.lock_commit_gate();
        self.inner
            .lock_state()
            .graph
            .scopes_mut()
            .ensure_child(id, kind, parent)
    }

    pub fn open_child_scope(
        &self,
        id: ScopeId,
        kind: ScopeKind,
        parent: ScopeId,
    ) -> Result<CapabilityScope, ScopeGraphError> {
        let _commit_guard = self.inner.lock_commit_gate();
        let mut state = self.inner.lock_state();
        state
            .graph
            .scopes_mut()
            .ensure_child(id.clone(), kind, parent)?;
        *state.scope_handles.entry(id.clone()).or_default() += 1;
        Ok(CapabilityScope {
            runtime: self.clone(),
            id,
        })
    }

    pub fn graph(&self) -> CapabilityGraph {
        self.inner.lock_state().graph.clone()
    }

    pub fn acquire(
        &self,
        request_scope: &ScopeId,
        capability: &CapabilityId,
    ) -> Option<CapabilityLease> {
        let mut state = self.inner.lock_state();
        let manifest = state.graph.visible(request_scope, capability)?;
        if !state.graph.scopes().can_see(request_scope, &manifest.scope) {
            return None;
        }
        let generation_id = *state.active_generations.get(&ScopedCapabilityKey {
            id: capability.clone(),
            scope: manifest.scope.clone(),
        })?;
        let generation = state.generations.get_mut(&generation_id)?;
        if generation.lifecycle != CapabilityLifecycle::Active {
            return None;
        }
        generation.leases += 1;
        Some(CapabilityLease::new(
            Arc::clone(&self.inner),
            capability.clone(),
            generation_id,
        ))
    }

    pub fn acquire_typed<T>(
        &self,
        request_scope: &ScopeId,
        capability: &CapabilityId,
    ) -> Result<Option<TypedCapability<T>>, CapabilityPayloadError>
    where
        T: Any + Send + Sync + 'static,
    {
        let mut state = self.inner.lock_state();
        let Some(manifest) = state.graph.visible(request_scope, capability) else {
            return Ok(None);
        };
        if !state.graph.scopes().can_see(request_scope, &manifest.scope) {
            return Ok(None);
        }
        let Some(generation_id) = state
            .active_generations
            .get(&ScopedCapabilityKey {
                id: capability.clone(),
                scope: manifest.scope.clone(),
            })
            .copied()
        else {
            return Ok(None);
        };
        let Some(generation) = state.generations.get_mut(&generation_id) else {
            return Ok(None);
        };
        if generation.lifecycle != CapabilityLifecycle::Active {
            return Ok(None);
        }
        let Some(payload) = generation.payloads.get(capability).cloned() else {
            return Err(CapabilityPayloadError::MissingPayload {
                capability: capability.clone(),
                generation: generation_id,
            });
        };
        let value =
            Arc::downcast::<T>(payload).map_err(|_| CapabilityPayloadError::TypeMismatch {
                capability: capability.clone(),
                generation: generation_id,
                requested_type: type_name::<T>(),
            })?;
        generation.leases += 1;
        Ok(Some(TypedCapability::new(
            value,
            CapabilityLease::new(Arc::clone(&self.inner), capability.clone(), generation_id),
        )))
    }

    pub fn snapshot(&self) -> RuntimeSnapshot {
        let state = self.inner.lock_state();
        let capabilities = state
            .graph
            .manifests()
            .filter_map(|manifest| {
                let generation = *state.active_generations.get(&ScopedCapabilityKey {
                    id: manifest.id.clone(),
                    scope: manifest.scope.clone(),
                })?;
                let lifecycle = state.generations.get(&generation)?.lifecycle;
                Some(CapabilitySnapshot {
                    id: manifest.id.clone(),
                    kind: manifest.kind,
                    owner: manifest.owner.clone(),
                    scope: manifest.scope.clone(),
                    generation,
                    lifecycle,
                })
            })
            .collect();
        let generations = state
            .generations
            .iter()
            .map(|(id, generation)| GenerationSnapshot {
                id: *id,
                owner: generation.owner.clone(),
                scope: generation.scope.clone(),
                lifecycle: generation.lifecycle,
                leases: generation.leases,
                capabilities: generation.capabilities.clone(),
                disposal_failures: generation.disposal_failures.clone(),
            })
            .collect();
        RuntimeSnapshot {
            capabilities,
            generations,
        }
    }

    fn retire_scope(&self, scope: &ScopeId) {
        let commit_guard = self.inner.lock_commit_gate();
        let immediate_disposal = {
            let mut state = self.inner.lock_state();
            let Some(handles) = state.scope_handles.get_mut(scope) else {
                return;
            };
            *handles = handles.saturating_sub(1);
            if *handles > 0 {
                return;
            }
            state.scope_handles.remove(scope);
            let retired_capabilities = state
                .graph
                .manifests()
                .filter(|manifest| &manifest.scope == scope)
                .map(|manifest| manifest.id.clone())
                .collect::<Vec<_>>();
            let retired_generations = retired_capabilities
                .iter()
                .filter_map(|id| {
                    state.active_generations.remove(&ScopedCapabilityKey {
                        id: id.clone(),
                        scope: scope.clone(),
                    })
                })
                .collect::<BTreeSet<_>>();
            for id in retired_capabilities {
                state.graph.remove_in_scope(&id, scope);
            }

            let mut immediate_disposal = Vec::new();
            for generation_id in retired_generations {
                if let Some(record) = state.generations.get_mut(&generation_id) {
                    record.lifecycle = CapabilityLifecycle::Quiescing;
                    if record.leases == 0 {
                        record.lifecycle = CapabilityLifecycle::Retired;
                        record.payloads.clear();
                        immediate_disposal
                            .push((generation_id, std::mem::take(&mut record.disposers)));
                    }
                }
            }
            immediate_disposal
        };
        drop(commit_guard);

        for (generation_id, disposers) in immediate_disposal {
            self.inner.schedule_disposal(generation_id, disposers);
        }
    }
}

pub struct CapabilityScope {
    runtime: CapabilityRuntime,
    id: ScopeId,
}

impl CapabilityScope {
    pub fn id(&self) -> &ScopeId {
        &self.id
    }

    pub fn runtime(&self) -> CapabilityRuntime {
        self.runtime.clone()
    }

    pub fn begin_transaction(&self, owner: CapabilityOwnerId) -> CapabilityTransaction {
        self.runtime.begin_transaction(owner, self.id.clone())
    }

    pub fn acquire_typed<T>(
        &self,
        capability: &CapabilityId,
    ) -> Result<Option<TypedCapability<T>>, CapabilityPayloadError>
    where
        T: Any + Send + Sync + 'static,
    {
        self.runtime.acquire_typed(&self.id, capability)
    }
}

impl fmt::Debug for CapabilityScope {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CapabilityScope")
            .field("id", &self.id)
            .finish()
    }
}

impl Drop for CapabilityScope {
    fn drop(&mut self) {
        self.runtime.retire_scope(&self.id);
    }
}

pub struct CapabilityTransaction {
    runtime: CapabilityRuntime,
    owner: CapabilityOwnerId,
    scope: ScopeId,
    staged: BTreeMap<CapabilityId, StagedCapability>,
}

impl CapabilityTransaction {
    pub fn stage(
        &mut self,
        manifest: CapabilityManifest,
        activate: impl FnOnce() -> Result<CapabilityDisposer, String> + Send + 'static,
    ) -> Result<(), CapabilityCommitError> {
        self.stage_typed(manifest, move || activate().map(|disposer| ((), disposer)))
    }

    pub fn stage_typed<T>(
        &mut self,
        manifest: CapabilityManifest,
        activate: impl FnOnce() -> Result<(T, CapabilityDisposer), String> + Send + 'static,
    ) -> Result<(), CapabilityCommitError>
    where
        T: Any + Send + Sync + 'static,
    {
        if manifest.owner != self.owner {
            return Err(CapabilityCommitError::OwnerMismatch {
                transaction_owner: self.owner.clone(),
                manifest_owner: manifest.owner,
            });
        }
        if manifest.scope != self.scope {
            return Err(CapabilityCommitError::ScopeMismatch {
                transaction_scope: self.scope.clone(),
                manifest_scope: manifest.scope,
            });
        }
        if self.staged.contains_key(&manifest.id) {
            return Err(CapabilityCommitError::DuplicateStagedCapability {
                capability: manifest.id,
            });
        }
        self.staged.insert(
            manifest.id.clone(),
            StagedCapability {
                manifest,
                activate: Box::new(move || {
                    activate().map(|(payload, disposer)| ActivatedCapability {
                        payload: Arc::new(payload),
                        disposer,
                    })
                }),
            },
        );
        Ok(())
    }

    pub fn commit(self) -> Result<CapabilityCommitReport, CapabilityCommitError> {
        let commit_guard = self.runtime.inner.lock_commit_gate();
        let mut candidate = self.runtime.graph();
        let replaced = candidate
            .manifests()
            .filter(|manifest| manifest.owner == self.owner && manifest.scope == self.scope)
            .map(|manifest| manifest.id.clone())
            .collect::<Vec<_>>();
        for id in &replaced {
            candidate.remove_in_scope(id, &self.scope);
        }
        for staged in self.staged.values() {
            candidate.insert(staged.manifest.clone())?;
        }
        candidate.validate()?;

        let staged_ids = self.staged.keys().cloned().collect::<BTreeSet<_>>();
        let activation_order = candidate
            .resolve(&self.scope, staged_ids.iter().cloned())?
            .ordered_ids()
            .iter()
            .filter(|id| staged_ids.contains(*id))
            .cloned()
            .collect::<Vec<_>>();

        let mut staged = self.staged;
        let mut activated = Vec::new();
        let mut payloads = BTreeMap::new();
        let mut disposers = Vec::new();
        for id in &activation_order {
            let Some(staged_capability) = staged.remove(id) else {
                let rollback_failures = dispose_all(disposers);
                return Err(CapabilityCommitError::InternalInvariant {
                    reason: format!("activation order referenced unstaged capability {id}"),
                    rollback_failures,
                });
            };
            match (staged_capability.activate)() {
                Ok(activation) => {
                    activated.push(id.clone());
                    payloads.insert(id.clone(), activation.payload);
                    disposers.push(activation.disposer);
                }
                Err(reason) => {
                    let rollback_failures = dispose_all(disposers);
                    return Err(CapabilityCommitError::ActivationFailed {
                        capability: id.clone(),
                        reason,
                        rollback_failures,
                    });
                }
            }
        }

        let (generation, retired, immediate_disposal) = {
            let mut state = self.runtime.inner.lock_state();
            let generation = GenerationId::new(state.next_generation);
            state.next_generation += 1;

            let retired = replaced
                .iter()
                .filter_map(|id| {
                    state.active_generations.remove(&ScopedCapabilityKey {
                        id: id.clone(),
                        scope: self.scope.clone(),
                    })
                })
                .collect::<BTreeSet<_>>();
            for id in &activated {
                state.active_generations.insert(
                    ScopedCapabilityKey {
                        id: id.clone(),
                        scope: self.scope.clone(),
                    },
                    generation,
                );
            }
            state.graph = candidate;
            state.generations.insert(
                generation,
                GenerationRecord {
                    owner: self.owner.clone(),
                    scope: self.scope.clone(),
                    lifecycle: if activated.is_empty() {
                        CapabilityLifecycle::Disposed
                    } else {
                        CapabilityLifecycle::Active
                    },
                    leases: 0,
                    capabilities: activated.clone(),
                    payloads,
                    disposers,
                    disposal_failures: Vec::new(),
                },
            );

            let mut immediate_disposal = Vec::new();
            for retired_generation in &retired {
                if let Some(record) = state.generations.get_mut(retired_generation) {
                    record.lifecycle = CapabilityLifecycle::Quiescing;
                    if record.leases == 0 {
                        record.lifecycle = CapabilityLifecycle::Retired;
                        record.payloads.clear();
                        immediate_disposal
                            .push((*retired_generation, std::mem::take(&mut record.disposers)));
                    }
                }
            }
            (
                generation,
                retired.into_iter().collect::<Vec<_>>(),
                immediate_disposal,
            )
        };
        drop(commit_guard);

        let mut disposal_failures = Vec::new();
        for (retired_generation, retired_disposers) in immediate_disposal {
            let failures = dispose_all(retired_disposers);
            disposal_failures.extend(
                failures
                    .iter()
                    .map(|failure| format!("generation {retired_generation}: {failure}")),
            );
            let mut state = self.runtime.inner.lock_state();
            if let Some(record) = state.generations.get_mut(&retired_generation) {
                record.disposal_failures.extend(failures);
                record.lifecycle = CapabilityLifecycle::Disposed;
            }
        }

        Ok(CapabilityCommitReport {
            generation,
            owner: self.owner,
            scope: self.scope,
            activated,
            retired,
            disposal_failures,
        })
    }
}

fn dispose_all(disposers: Vec<CapabilityDisposer>) -> Vec<String> {
    disposers
        .into_iter()
        .rev()
        .filter_map(
            |dispose| match std::panic::catch_unwind(AssertUnwindSafe(dispose)) {
                Ok(result) => result.err(),
                Err(_) => Some("capability disposer panicked".to_string()),
            },
        )
        .collect()
}

impl RuntimeInner {
    fn lock_commit_gate(&self) -> MutexGuard<'_, ()> {
        self.commit_gate
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn lock_state(&self) -> MutexGuard<'_, RuntimeState> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    pub(crate) fn lease_lifecycle(&self, generation: GenerationId) -> CapabilityLifecycle {
        self.lock_state()
            .generations
            .get(&generation)
            .map_or(CapabilityLifecycle::Disposed, |record| record.lifecycle)
    }

    pub(crate) fn clone_lease(&self, generation: GenerationId) -> bool {
        let mut state = self.lock_state();
        let Some(record) = state.generations.get_mut(&generation) else {
            return false;
        };
        if matches!(
            record.lifecycle,
            CapabilityLifecycle::Disposed | CapabilityLifecycle::Retired
        ) {
            return false;
        }
        record.leases += 1;
        true
    }

    pub(crate) fn release_lease(&self, generation: GenerationId) {
        let disposers = {
            let mut state = self.lock_state();
            let Some(record) = state.generations.get_mut(&generation) else {
                return;
            };
            debug_assert!(record.leases > 0, "lease count must not underflow");
            record.leases = record.leases.saturating_sub(1);
            if record.leases == 0 && record.lifecycle == CapabilityLifecycle::Quiescing {
                record.lifecycle = CapabilityLifecycle::Retired;
                record.payloads.clear();
                Some(std::mem::take(&mut record.disposers))
            } else {
                None
            }
        };
        let Some(disposers) = disposers else {
            return;
        };
        self.schedule_disposal(generation, disposers);
    }

    fn schedule_disposal(&self, generation: GenerationId, disposers: Vec<CapabilityDisposer>) {
        if disposers.is_empty() {
            if let Some(record) = self.lock_state().generations.get_mut(&generation) {
                record.lifecycle = CapabilityLifecycle::Disposed;
            }
            return;
        }
        if let Err(error) = self.disposal_tx.send(DisposalJob {
            generation,
            disposers,
        }) {
            let job = error.0;
            let _ = thread::Builder::new()
                .name("praxis-capability-disposal-fallback".to_string())
                .stack_size(DISPOSAL_REAPER_STACK_BYTES)
                .spawn(move || {
                    let _ = dispose_all(job.disposers);
                });
        }
    }
}

fn spawn_disposal_reaper(runtime: Weak<RuntimeInner>, jobs: mpsc::Receiver<DisposalJob>) {
    thread::Builder::new()
        .name("praxis-capability-reaper".to_string())
        .stack_size(DISPOSAL_REAPER_STACK_BYTES)
        .spawn(move || {
            while let Ok(job) = jobs.recv() {
                let failures = dispose_all(job.disposers);
                let Some(runtime) = runtime.upgrade() else {
                    continue;
                };
                let mut state = runtime.lock_state();
                if let Some(record) = state.generations.get_mut(&job.generation) {
                    record.disposal_failures.extend(failures);
                    record.lifecycle = CapabilityLifecycle::Disposed;
                }
            }
        })
        .expect("failed to start capability disposal reaper");
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilityCommitReport {
    pub generation: GenerationId,
    pub owner: CapabilityOwnerId,
    pub scope: ScopeId,
    pub activated: Vec<CapabilityId>,
    pub retired: Vec<GenerationId>,
    pub disposal_failures: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilitySnapshot {
    pub id: CapabilityId,
    pub kind: CapabilityKind,
    pub owner: CapabilityOwnerId,
    pub scope: ScopeId,
    pub generation: GenerationId,
    pub lifecycle: CapabilityLifecycle,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenerationSnapshot {
    pub id: GenerationId,
    pub owner: CapabilityOwnerId,
    pub scope: ScopeId,
    pub lifecycle: CapabilityLifecycle,
    pub leases: usize,
    pub capabilities: Vec<CapabilityId>,
    pub disposal_failures: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeSnapshot {
    pub capabilities: Vec<CapabilitySnapshot>,
    pub generations: Vec<GenerationSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CapabilityCommitError {
    Graph(CapabilityGraphError),
    OwnerMismatch {
        transaction_owner: CapabilityOwnerId,
        manifest_owner: CapabilityOwnerId,
    },
    ScopeMismatch {
        transaction_scope: ScopeId,
        manifest_scope: ScopeId,
    },
    DuplicateStagedCapability {
        capability: CapabilityId,
    },
    ActivationFailed {
        capability: CapabilityId,
        reason: String,
        rollback_failures: Vec<String>,
    },
    InternalInvariant {
        reason: String,
        rollback_failures: Vec<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CapabilityPayloadError {
    MissingPayload {
        capability: CapabilityId,
        generation: GenerationId,
    },
    TypeMismatch {
        capability: CapabilityId,
        generation: GenerationId,
        requested_type: &'static str,
    },
}

impl fmt::Display for CapabilityPayloadError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for CapabilityPayloadError {}

impl From<CapabilityGraphError> for CapabilityCommitError {
    fn from(value: CapabilityGraphError) -> Self {
        Self::Graph(value)
    }
}

impl fmt::Display for CapabilityCommitError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for CapabilityCommitError {}
