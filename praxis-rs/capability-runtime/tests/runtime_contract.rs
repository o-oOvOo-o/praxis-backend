#![allow(clippy::expect_used, clippy::unwrap_used)]

use praxis_capability_runtime::CapabilityCommitError;
use praxis_capability_runtime::CapabilityGraph;
use praxis_capability_runtime::CapabilityGraphError;
use praxis_capability_runtime::CapabilityId;
use praxis_capability_runtime::CapabilityKind;
use praxis_capability_runtime::CapabilityLifecycle;
use praxis_capability_runtime::CapabilityManifest;
use praxis_capability_runtime::CapabilityOwnerId;
use praxis_capability_runtime::CapabilityRuntime;
use praxis_capability_runtime::ScopeGraph;
use praxis_capability_runtime::ScopeId;
use praxis_capability_runtime::ScopeKind;
use std::sync::Arc;
use std::sync::Mutex;

fn capability_id(value: &str) -> CapabilityId {
    CapabilityId::new(value).expect("valid capability id")
}

fn owner_id(value: &str) -> CapabilityOwnerId {
    CapabilityOwnerId::new(value).expect("valid owner id")
}

fn scope_id(value: &str) -> ScopeId {
    ScopeId::new(value).expect("valid scope id")
}

fn manifest(id: &str, owner: &str, scope: &ScopeId, dependencies: &[&str]) -> CapabilityManifest {
    CapabilityManifest::new(
        capability_id(id),
        CapabilityKind::Service,
        owner_id(owner),
        scope.clone(),
    )
    .with_dependencies(dependencies.iter().copied().map(capability_id))
}

fn process_scopes() -> (ScopeGraph, ScopeId, ScopeId, ScopeId) {
    let process = scope_id("process");
    let workspace = scope_id("workspace:main");
    let sibling = scope_id("workspace:sibling");
    let mut scopes = ScopeGraph::new();
    scopes
        .add_root(process.clone(), ScopeKind::Process)
        .expect("process root");
    scopes
        .add_child(workspace.clone(), ScopeKind::Workspace, process.clone())
        .expect("workspace scope");
    scopes
        .add_child(sibling.clone(), ScopeKind::Workspace, process.clone())
        .expect("sibling scope");
    (scopes, process, workspace, sibling)
}

#[test]
fn typed_ids_reject_blank_or_untrimmed_values() {
    assert!(CapabilityId::new("").is_err());
    assert!(CapabilityOwnerId::new(" plugin").is_err());
    assert!(ScopeId::new("thread:1 ").is_err());
}

#[test]
fn dependency_resolution_is_stable_and_dependencies_first() {
    let (scopes, process, workspace, _) = process_scopes();
    let mut graph = CapabilityGraph::new(scopes);
    graph
        .insert(manifest("transport", "core", &process, &[]))
        .unwrap();
    graph
        .insert(manifest("tools", "tools", &workspace, &["transport"]))
        .unwrap();
    graph
        .insert(manifest("thread", "thread", &workspace, &["tools"]))
        .unwrap();

    let resolved = graph
        .resolve(&workspace, [capability_id("thread")])
        .expect("resolved closure");

    assert_eq!(
        resolved.ordered_ids(),
        &[
            capability_id("transport"),
            capability_id("tools"),
            capability_id("thread"),
        ]
    );
}

#[test]
fn graph_rejects_missing_dependencies_cycles_and_conflicts() {
    let (scopes, process, _, _) = process_scopes();

    let mut missing = CapabilityGraph::new(scopes.clone());
    missing
        .insert(manifest("thread", "thread", &process, &["tools"]))
        .unwrap();
    assert!(matches!(
        missing.validate(),
        Err(CapabilityGraphError::MissingDependency { .. })
    ));

    let mut cyclic = CapabilityGraph::new(scopes.clone());
    cyclic
        .insert(manifest("a", "owner-a", &process, &["b"]))
        .unwrap();
    cyclic
        .insert(manifest("b", "owner-b", &process, &["a"]))
        .unwrap();
    assert!(matches!(
        cyclic.validate(),
        Err(CapabilityGraphError::DependencyCycle { .. })
    ));

    let mut conflicted = CapabilityGraph::new(scopes);
    conflicted
        .insert(manifest("a", "owner-a", &process, &[]).with_conflicts([capability_id("b")]))
        .unwrap();
    conflicted
        .insert(manifest("b", "owner-b", &process, &[]))
        .unwrap();
    assert!(matches!(
        conflicted.validate(),
        Err(CapabilityGraphError::Conflict { .. })
    ));
}

#[test]
fn conflicts_are_scoped_to_capabilities_that_can_coexist() {
    let (scopes, _, workspace, sibling) = process_scopes();
    let mut graph = CapabilityGraph::new(scopes);
    graph
        .insert(
            manifest("workspace-tools", "owner-a", &workspace, &[])
                .with_conflicts([capability_id("sibling-tools")]),
        )
        .unwrap();
    graph
        .insert(manifest("sibling-tools", "owner-b", &sibling, &[]))
        .unwrap();

    graph
        .validate()
        .expect("sibling capabilities cannot coexist in one request scope");
}

#[test]
fn ancestor_capabilities_are_visible_but_sibling_capabilities_are_isolated() {
    let (scopes, process, workspace, sibling) = process_scopes();
    let mut graph = CapabilityGraph::new(scopes);
    graph
        .insert(manifest("transport", "core", &process, &[]))
        .unwrap();
    graph
        .insert(manifest(
            "workspace-tools",
            "tools",
            &workspace,
            &["transport"],
        ))
        .unwrap();
    graph
        .insert(manifest(
            "sibling-thread",
            "thread",
            &sibling,
            &["workspace-tools"],
        ))
        .unwrap();

    assert!(
        graph
            .resolve(&workspace, [capability_id("workspace-tools")])
            .is_ok()
    );
    assert!(matches!(
        graph.validate(),
        Err(CapabilityGraphError::InvisibleDependency { .. })
    ));
}

#[test]
fn failed_activation_rolls_back_in_reverse_dependency_order() {
    let (scopes, process, _, _) = process_scopes();
    let runtime = CapabilityRuntime::new(scopes);
    let events = Arc::new(Mutex::new(Vec::new()));
    let mut transaction = runtime.begin_transaction(owner_id("plugin"), process.clone());

    for id in ["base", "middle"] {
        let events_for_activate = Arc::clone(&events);
        let events_for_dispose = Arc::clone(&events);
        let id_for_activate = id.to_owned();
        let id_for_dispose = id.to_owned();
        transaction
            .stage(
                manifest(
                    id,
                    "plugin",
                    &process,
                    if id == "middle" { &["base"] } else { &[] },
                ),
                move || {
                    events_for_activate
                        .lock()
                        .unwrap()
                        .push(format!("activate:{id_for_activate}"));
                    Ok(Box::new(move || {
                        events_for_dispose
                            .lock()
                            .unwrap()
                            .push(format!("dispose:{id_for_dispose}"));
                        Ok(())
                    }))
                },
            )
            .unwrap();
    }
    transaction
        .stage(manifest("top", "plugin", &process, &["middle"]), || {
            Err("injected activation failure".to_owned())
        })
        .unwrap();

    assert!(matches!(
        transaction.commit(),
        Err(CapabilityCommitError::ActivationFailed { .. })
    ));
    assert_eq!(
        *events.lock().unwrap(),
        [
            "activate:base",
            "activate:middle",
            "dispose:middle",
            "dispose:base",
        ]
    );
    assert!(runtime.snapshot().capabilities.is_empty());
}

#[test]
fn owner_transaction_is_atomic_and_old_generation_waits_for_lease_release() {
    let (scopes, process, workspace, _) = process_scopes();
    let runtime = CapabilityRuntime::new(scopes);
    let events = Arc::new(Mutex::new(Vec::new()));

    let old_events = Arc::clone(&events);
    let mut initial = runtime.begin_transaction(owner_id("tools"), workspace.clone());
    initial
        .stage(manifest("tools", "tools", &workspace, &[]), move || {
            Ok(Box::new(move || {
                old_events.lock().unwrap().push("dispose:old".to_owned());
                Ok(())
            }))
        })
        .unwrap();
    let initial_report = initial.commit().expect("initial commit");
    let lease = runtime
        .acquire(&workspace, &capability_id("tools"))
        .expect("active lease");

    let new_events = Arc::clone(&events);
    let mut replacement = runtime.begin_transaction(owner_id("tools"), workspace.clone());
    replacement
        .stage(manifest("tools", "tools", &workspace, &[]), move || {
            Ok(Box::new(move || {
                new_events.lock().unwrap().push("dispose:new".to_owned());
                Ok(())
            }))
        })
        .unwrap();
    let replacement_report = replacement.commit().expect("replacement commit");

    assert_ne!(initial_report.generation, replacement_report.generation);
    assert_eq!(
        runtime
            .acquire(&workspace, &capability_id("tools"))
            .unwrap()
            .generation_id(),
        replacement_report.generation
    );
    assert!(events.lock().unwrap().is_empty());
    assert_eq!(lease.lifecycle(), CapabilityLifecycle::Quiescing);

    drop(lease);

    assert_eq!(*events.lock().unwrap(), ["dispose:old"]);
    assert!(runtime.snapshot().generations.iter().any(|generation| {
        generation.id == initial_report.generation
            && generation.lifecycle == CapabilityLifecycle::Disposed
    }));

    let unload = runtime
        .begin_transaction(owner_id("tools"), workspace)
        .commit()
        .expect("empty owner transaction unloads the owner");
    assert_eq!(unload.activated, Vec::<CapabilityId>::new());
    assert_eq!(*events.lock().unwrap(), ["dispose:old", "dispose:new"]);

    // The process scope remains a valid request scope even with no capabilities.
    assert!(
        runtime
            .graph()
            .resolve(&process, Vec::<CapabilityId>::new())
            .is_ok()
    );
}

#[test]
fn failed_replacement_keeps_the_previous_generation_active() {
    let (scopes, _, workspace, _) = process_scopes();
    let runtime = CapabilityRuntime::new(scopes);
    let mut initial = runtime.begin_transaction(owner_id("tools"), workspace.clone());
    initial
        .stage(manifest("tools", "tools", &workspace, &[]), || {
            Ok(Box::new(|| Ok(())))
        })
        .unwrap();
    let initial_report = initial.commit().expect("initial commit");

    let mut replacement = runtime.begin_transaction(owner_id("tools"), workspace.clone());
    replacement
        .stage(manifest("tools", "tools", &workspace, &[]), || {
            Err("candidate failed validation".to_owned())
        })
        .unwrap();
    assert!(matches!(
        replacement.commit(),
        Err(CapabilityCommitError::ActivationFailed { .. })
    ));

    let lease = runtime
        .acquire(&workspace, &capability_id("tools"))
        .expect("previous generation remains active");
    assert_eq!(lease.generation_id(), initial_report.generation);
    assert_eq!(lease.lifecycle(), CapabilityLifecycle::Active);
}

#[test]
fn cloned_leases_delay_disposal_until_the_last_clone_drops() {
    let (scopes, _, workspace, _) = process_scopes();
    let runtime = CapabilityRuntime::new(scopes);
    let events = Arc::new(Mutex::new(Vec::new()));
    let dispose_events = Arc::clone(&events);
    let mut initial = runtime.begin_transaction(owner_id("tools"), workspace.clone());
    initial
        .stage(manifest("tools", "tools", &workspace, &[]), move || {
            Ok(Box::new(move || {
                dispose_events.lock().unwrap().push("disposed".to_owned());
                Ok(())
            }))
        })
        .unwrap();
    initial.commit().unwrap();

    let first = runtime
        .acquire(&workspace, &capability_id("tools"))
        .unwrap();
    let second = first.clone();
    runtime
        .begin_transaction(owner_id("tools"), workspace)
        .commit()
        .unwrap();

    drop(first);
    assert!(events.lock().unwrap().is_empty());
    drop(second);
    assert_eq!(*events.lock().unwrap(), ["disposed"]);
}

#[test]
fn runtime_registers_dynamic_child_scopes_idempotently() {
    let process = scope_id("process");
    let runtime =
        CapabilityRuntime::new(ScopeGraph::single_root(process.clone(), ScopeKind::Process));
    let thread = scope_id("thread:one");

    runtime
        .ensure_child_scope(thread.clone(), ScopeKind::Thread, process.clone())
        .expect("first registration");
    runtime
        .ensure_child_scope(thread.clone(), ScopeKind::Thread, process)
        .expect("same registration is idempotent");

    assert!(runtime.graph().scopes().contains(&thread));
}

#[test]
fn typed_payload_is_published_atomically_and_held_by_its_lease() {
    let (scopes, _, workspace, _) = process_scopes();
    let runtime = CapabilityRuntime::new(scopes);
    let capability = capability_id("hooks");
    let mut transaction = runtime.begin_transaction(owner_id("core-hooks"), workspace.clone());
    transaction
        .stage_typed(manifest("hooks", "core-hooks", &workspace, &[]), || {
            Ok((String::from("published hooks"), Box::new(|| Ok(()))))
        })
        .expect("stage typed hooks");
    let report = transaction.commit().expect("publish hooks");

    let hooks = runtime
        .acquire_typed::<String>(&workspace, &capability)
        .expect("payload type matches")
        .expect("hooks are active");

    assert_eq!(hooks.value(), "published hooks");
    assert_eq!(hooks.lease().generation_id(), report.generation);
}

#[test]
fn dropping_scope_quiesces_then_disposes_typed_payload_after_lease_release() {
    let process = ScopeId::process();
    let runtime =
        CapabilityRuntime::new(ScopeGraph::single_root(process.clone(), ScopeKind::Process));
    let scope = runtime
        .open_child_scope(scope_id("thread:guarded"), ScopeKind::Thread, process)
        .expect("thread scope");
    let capability = capability_id("hooks");
    let mut transaction = scope.begin_transaction(owner_id("core-hooks"));
    transaction
        .stage_typed(manifest("hooks", "core-hooks", scope.id(), &[]), || {
            Ok((String::from("hooks"), Box::new(|| Ok(()))))
        })
        .expect("stage hooks");
    let generation = transaction.commit().expect("commit hooks").generation;
    let hooks = scope
        .acquire_typed::<String>(&capability)
        .expect("typed payload")
        .expect("active hooks");

    drop(scope);
    assert_eq!(hooks.lease().lifecycle(), CapabilityLifecycle::Quiescing);
    drop(hooks);

    assert!(runtime.snapshot().generations.iter().any(|snapshot| {
        snapshot.id == generation && snapshot.lifecycle == CapabilityLifecycle::Disposed
    }));
}

#[test]
fn sibling_scopes_can_publish_the_same_logical_capability_id() {
    let process = ScopeId::process();
    let runtime =
        CapabilityRuntime::new(ScopeGraph::single_root(process.clone(), ScopeKind::Process));
    let first = runtime
        .open_child_scope(scope_id("thread:first"), ScopeKind::Thread, process.clone())
        .expect("first thread");
    let second = runtime
        .open_child_scope(scope_id("thread:second"), ScopeKind::Thread, process)
        .expect("second thread");
    let capability = capability_id("hooks");

    for (scope, value) in [(&first, "first"), (&second, "second")] {
        let mut transaction = scope.begin_transaction(owner_id("core-hooks"));
        transaction
            .stage_typed(
                manifest("hooks", "core-hooks", scope.id(), &[]),
                move || Ok((String::from(value), Box::new(|| Ok(())))),
            )
            .expect("stage sibling hooks");
        transaction.commit().expect("commit sibling hooks");
    }

    assert_eq!(
        first
            .acquire_typed::<String>(&capability)
            .expect("first typed payload")
            .expect("first hooks")
            .value(),
        "first"
    );
    assert_eq!(
        second
            .acquire_typed::<String>(&capability)
            .expect("second typed payload")
            .expect("second hooks")
            .value(),
        "second"
    );
}

#[test]
fn scope_disposal_waits_for_the_final_scope_handle() {
    let process = ScopeId::process();
    let runtime =
        CapabilityRuntime::new(ScopeGraph::single_root(process.clone(), ScopeKind::Process));
    let scope_id = scope_id("thread:shared-handle");
    let first = runtime
        .open_child_scope(scope_id.clone(), ScopeKind::Thread, process.clone())
        .expect("first handle");
    let second = runtime
        .open_child_scope(scope_id, ScopeKind::Thread, process)
        .expect("second handle");
    let capability = capability_id("hooks");
    let mut transaction = first.begin_transaction(owner_id("core-hooks"));
    transaction
        .stage_typed(manifest("hooks", "core-hooks", first.id(), &[]), || {
            Ok((String::from("hooks"), Box::new(|| Ok(()))))
        })
        .expect("stage hooks");
    transaction.commit().expect("commit hooks");

    drop(first);
    assert!(
        second
            .acquire_typed::<String>(&capability)
            .expect("typed payload")
            .is_some()
    );
}
