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
use arc_swap::ArcSwap;
use std::any::Any;
use std::any::type_name;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::fmt;
use std::panic::AssertUnwindSafe;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::MutexGuard;
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

pub(crate) struct GenerationRecord {
    id: GenerationId,
    owner: CapabilityOwnerId,
    scope: ScopeId,
    capabilities: Vec<CapabilityId>,
    mutable: Mutex<GenerationMutable>,
    disposal_tx: mpsc::Sender<DisposalJob>,
}

struct GenerationMutable {
    lifecycle: CapabilityLifecycle,
    leases: usize,
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
    routes: ArcSwap<RouteSnapshot>,
    disposal_tx: mpsc::Sender<DisposalJob>,
}

struct DisposalJob {
    generation: Arc<GenerationRecord>,
    disposers: Vec<CapabilityDisposer>,
}

struct RuntimeState {
    revision: u64,
    next_generation: u64,
    graph: CapabilityGraph,
    scope_handles: BTreeMap<ScopeId, usize>,
    active_generations: BTreeMap<ScopedCapabilityKey, GenerationId>,
    generations: BTreeMap<GenerationId, Arc<GenerationRecord>>,
}

struct RouteSnapshot {
    scopes: ScopeGraph,
    routes: BTreeMap<ScopedCapabilityKey, Arc<GenerationRecord>>,
}

impl RouteSnapshot {
    fn from_state(state: &RuntimeState) -> Self {
        let routes = state
            .active_generations
            .iter()
            .filter_map(|(key, generation)| {
                state
                    .generations
                    .get(generation)
                    .map(|record| (key.clone(), Arc::clone(record)))
            })
            .collect();
        Self {
            scopes: state.graph.scopes().clone(),
            routes,
        }
    }

    fn visible(
        &self,
        request_scope: &ScopeId,
        capability: &CapabilityId,
    ) -> Option<Arc<GenerationRecord>> {
        let mut current = Some(request_scope);
        while let Some(scope) = current {
            if let Some(generation) = self.routes.get(&ScopedCapabilityKey {
                id: capability.clone(),
                scope: scope.clone(),
            }) {
                return Some(Arc::clone(generation));
            }
            current = self.scopes.parent(scope);
        }
        None
    }
}

impl CapabilityRuntime {
    pub fn new(scopes: ScopeGraph) -> Self {
        let (disposal_tx, disposal_rx) = mpsc::channel();
        let state = RuntimeState {
            revision: 0,
            next_generation: 1,
            graph: CapabilityGraph::new(scopes),
            scope_handles: BTreeMap::new(),
            active_generations: BTreeMap::new(),
            generations: BTreeMap::new(),
        };
        let routes = ArcSwap::from_pointee(RouteSnapshot::from_state(&state));
        let inner = Arc::new(RuntimeInner {
            commit_gate: Mutex::new(()),
            state: Mutex::new(state),
            routes,
            disposal_tx,
        });
        spawn_disposal_reaper(disposal_rx);
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
        let mut state = self.inner.lock_state();
        state.graph.scopes_mut().ensure_child(id, kind, parent)?;
        state.revision = state.revision.wrapping_add(1);
        self.inner
            .routes
            .store(Arc::new(RouteSnapshot::from_state(&state)));
        Ok(())
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
        state.revision = state.revision.wrapping_add(1);
        self.inner
            .routes
            .store(Arc::new(RouteSnapshot::from_state(&state)));
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
        loop {
            let routes = self.inner.routes.load_full();
            let generation = routes.visible(request_scope, capability)?;
            if generation.try_acquire() {
                return Some(CapabilityLease::new(capability.clone(), generation));
            }
        }
    }

    pub fn acquire_typed<T>(
        &self,
        request_scope: &ScopeId,
        capability: &CapabilityId,
    ) -> Result<Option<TypedCapability<T>>, CapabilityPayloadError>
    where
        T: Any + Send + Sync + 'static,
    {
        let generation = loop {
            let routes = self.inner.routes.load_full();
            let Some(generation) = routes.visible(request_scope, capability) else {
                return Ok(None);
            };
            if generation.try_acquire() {
                break generation;
            }
        };
        let generation_id = generation.id();
        let Some(payload) = generation.payload(capability) else {
            generation.release_lease();
            return Err(CapabilityPayloadError::MissingPayload {
                capability: capability.clone(),
                generation: generation_id,
            });
        };
        let value = Arc::downcast::<T>(payload).map_err(|_| {
            generation.release_lease();
            CapabilityPayloadError::TypeMismatch {
                capability: capability.clone(),
                generation: generation_id,
                requested_type: type_name::<T>(),
            }
        })?;
        Ok(Some(TypedCapability::new(
            value,
            CapabilityLease::new(capability.clone(), generation),
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
                let lifecycle = state.generations.get(&generation)?.lifecycle();
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
            .map(|(id, generation)| {
                let (lifecycle, leases, disposal_failures) = generation.snapshot_state();
                GenerationSnapshot {
                    id: *id,
                    owner: generation.owner.clone(),
                    scope: generation.scope.clone(),
                    lifecycle,
                    leases,
                    capabilities: generation.capabilities.clone(),
                    disposal_failures,
                }
            })
            .collect();
        RuntimeSnapshot {
            capabilities,
            generations,
        }
    }

    fn retire_scope(&self, scope: &ScopeId) {
        let commit_guard = self.inner.lock_commit_gate();
        let (retired_generations, routes) = {
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
            let retired_generation_ids = retired_capabilities
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

            let retired_generations = retired_generation_ids
                .into_iter()
                .filter_map(|generation| state.generations.get(&generation).cloned())
                .collect::<Vec<_>>();
            state.revision = state.revision.wrapping_add(1);
            let routes = Arc::new(RouteSnapshot::from_state(&state));
            (retired_generations, routes)
        };
        for generation in &retired_generations {
            if let Some(disposers) = generation.begin_quiescing() {
                generation.schedule_disposal(disposers);
            }
        }
        self.inner.routes.store(routes);
        drop(commit_guard);
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
        let staged_ids = self.staged.keys().cloned().collect::<BTreeSet<_>>();
        let (candidate, replaced, activation_order, expected_revision) = {
            let _commit_guard = self.runtime.inner.lock_commit_gate();
            let state = self.runtime.inner.lock_state();
            let mut candidate = state.graph.clone();
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
            let activation_order = candidate
                .resolve(&self.scope, staged_ids.iter().cloned())?
                .ordered_ids()
                .iter()
                .filter(|id| staged_ids.contains(*id))
                .cloned()
                .collect::<Vec<_>>();
            (candidate, replaced, activation_order, state.revision)
        };

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

        let commit_guard = self.runtime.inner.lock_commit_gate();
        let (generation, retired, retired_records, routes) = {
            let mut state = self.runtime.inner.lock_state();
            if state.revision != expected_revision {
                let actual_revision = state.revision;
                drop(state);
                drop(commit_guard);
                let rollback_failures = dispose_all(disposers);
                return Err(CapabilityCommitError::ConcurrentMutation {
                    expected_revision,
                    actual_revision,
                    rollback_failures,
                });
            }
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
            let record = Arc::new(GenerationRecord {
                id: generation,
                owner: self.owner.clone(),
                scope: self.scope.clone(),
                capabilities: activated.clone(),
                mutable: Mutex::new(GenerationMutable {
                    lifecycle: if activated.is_empty() {
                        CapabilityLifecycle::Disposed
                    } else {
                        CapabilityLifecycle::Active
                    },
                    leases: 0,
                    payloads,
                    disposers,
                    disposal_failures: Vec::new(),
                }),
                disposal_tx: self.runtime.inner.disposal_tx.clone(),
            });
            state.generations.insert(generation, record);
            state.revision = state.revision.wrapping_add(1);

            let retired_records = retired
                .iter()
                .filter_map(|generation| state.generations.get(generation).cloned())
                .collect::<Vec<_>>();
            let routes = Arc::new(RouteSnapshot::from_state(&state));
            (
                generation,
                retired.into_iter().collect::<Vec<_>>(),
                retired_records,
                routes,
            )
        };
        let mut immediate_disposal = Vec::new();
        for record in retired_records {
            if let Some(disposers) = record.begin_quiescing() {
                immediate_disposal.push((record, disposers));
            }
        }
        self.runtime.inner.routes.store(routes);
        drop(commit_guard);

        let mut disposal_failures = Vec::new();
        for (record, retired_disposers) in immediate_disposal {
            let failures = dispose_all(retired_disposers);
            disposal_failures.extend(
                failures
                    .iter()
                    .map(|failure| format!("generation {}: {failure}", record.id())),
            );
            record.complete_disposal(failures);
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

impl GenerationRecord {
    pub(crate) fn id(&self) -> GenerationId {
        self.id
    }

    fn lock_mutable(&self) -> MutexGuard<'_, GenerationMutable> {
        self.mutable
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    pub(crate) fn lifecycle(&self) -> CapabilityLifecycle {
        self.lock_mutable().lifecycle
    }

    fn snapshot_state(&self) -> (CapabilityLifecycle, usize, Vec<String>) {
        let mutable = self.lock_mutable();
        (
            mutable.lifecycle,
            mutable.leases,
            mutable.disposal_failures.clone(),
        )
    }

    fn payload(&self, capability: &CapabilityId) -> Option<Arc<dyn Any + Send + Sync>> {
        self.lock_mutable().payloads.get(capability).cloned()
    }

    fn try_acquire(&self) -> bool {
        let mut mutable = self.lock_mutable();
        if mutable.lifecycle != CapabilityLifecycle::Active {
            return false;
        }
        mutable.leases = mutable
            .leases
            .checked_add(1)
            .expect("capability lease count overflow");
        true
    }

    pub(crate) fn retain_lease(&self) -> bool {
        let mut mutable = self.lock_mutable();
        if matches!(
            mutable.lifecycle,
            CapabilityLifecycle::Retired | CapabilityLifecycle::Disposed
        ) {
            return false;
        }
        mutable.leases = mutable
            .leases
            .checked_add(1)
            .expect("capability lease count overflow");
        true
    }

    fn begin_quiescing(&self) -> Option<Vec<CapabilityDisposer>> {
        let mut mutable = self.lock_mutable();
        if mutable.lifecycle == CapabilityLifecycle::Active {
            mutable.lifecycle = CapabilityLifecycle::Quiescing;
        }
        take_disposal_if_drained(&mut mutable)
    }

    pub(crate) fn release_lease(self: &Arc<Self>) {
        let disposers = {
            let mut mutable = self.lock_mutable();
            debug_assert!(mutable.leases > 0, "lease count must not underflow");
            mutable.leases = mutable.leases.saturating_sub(1);
            take_disposal_if_drained(&mut mutable)
        };
        if let Some(disposers) = disposers {
            self.schedule_disposal(disposers);
        }
    }

    fn schedule_disposal(self: &Arc<Self>, disposers: Vec<CapabilityDisposer>) {
        if disposers.is_empty() {
            self.complete_disposal(Vec::new());
            return;
        }
        if let Err(error) = self.disposal_tx.send(DisposalJob {
            generation: Arc::clone(self),
            disposers,
        }) {
            let job = error.0;
            let _ = thread::Builder::new()
                .name("praxis-capability-disposal-fallback".to_string())
                .stack_size(DISPOSAL_REAPER_STACK_BYTES)
                .spawn(move || {
                    let failures = dispose_all(job.disposers);
                    job.generation.complete_disposal(failures);
                });
        }
    }

    fn complete_disposal(&self, failures: Vec<String>) {
        let mut mutable = self.lock_mutable();
        mutable.disposal_failures.extend(failures);
        mutable.lifecycle = CapabilityLifecycle::Disposed;
    }
}

fn take_disposal_if_drained(mutable: &mut GenerationMutable) -> Option<Vec<CapabilityDisposer>> {
    if mutable.leases != 0 || mutable.lifecycle != CapabilityLifecycle::Quiescing {
        return None;
    }
    mutable.lifecycle = CapabilityLifecycle::Retired;
    mutable.payloads.clear();
    Some(std::mem::take(&mut mutable.disposers))
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
}

fn spawn_disposal_reaper(jobs: mpsc::Receiver<DisposalJob>) {
    thread::Builder::new()
        .name("praxis-capability-reaper".to_string())
        .stack_size(DISPOSAL_REAPER_STACK_BYTES)
        .spawn(move || {
            while let Ok(job) = jobs.recv() {
                let failures = dispose_all(job.disposers);
                job.generation.complete_disposal(failures);
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
    ConcurrentMutation {
        expected_revision: u64,
        actual_revision: u64,
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
