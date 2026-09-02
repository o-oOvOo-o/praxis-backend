//! Typed, scoped, transactional ownership for Praxis product capabilities.
//!
//! This crate is the lifecycle authority shared by Praxis, Harness, Metra,
//! Bevy-backed game runtimes, hot reload, and Cook packaging. Product-specific
//! registries are projections of this runtime rather than independent owners.
//!
//! # Contract
//!
//! - Every contribution has a stable [`CapabilityId`], [`CapabilityOwnerId`],
//!   [`ScopeId`], and [`GenerationId`].
//! - Dependencies must be visible from the dependent capability's scope and
//!   resolve in deterministic dependency-first order.
//! - A [`CapabilityTransaction`] validates a complete candidate graph and
//!   activates every staged contribution before atomically publishing it.
//! - Activation failure runs staged disposers in reverse dependency order and
//!   leaves the previously published graph unchanged.
//! - Replaced generations enter [`CapabilityLifecycle::Quiescing`]. Their
//!   disposers cannot run until the last cloneable [`CapabilityLease`] drops.
//!   Retirement is executed by the runtime reaper, never by the consumer that
//!   releases the final lease or scope handle.
//! - Packaging must consume [`CapabilityGraph::resolve`] so runtime activation
//!   and product dependency closure cannot diverge.
//!
//! The kernel intentionally does not depend on `praxis-plugin`. Plugin identity
//! is adapted to [`CapabilityOwnerId`] at the plugin carrier boundary, keeping
//! the lifecycle runtime reusable for built-ins, editor modules, game worlds,
//! and Cook roots without creating a second implementation.

mod graph;
mod ids;
mod lease;
mod manifest;
mod runtime;
mod scope;

pub use graph::CapabilityGraph;
pub use graph::CapabilityGraphError;
pub use graph::ResolvedCapabilityGraph;
pub use ids::CapabilityId;
pub use ids::CapabilityOwnerId;
pub use ids::GenerationId;
pub use ids::IdError;
pub use ids::ScopeId;
pub use lease::CapabilityLease;
pub use lease::TypedCapability;
pub use manifest::CapabilityKind;
pub use manifest::CapabilityManifest;
pub use runtime::CapabilityActivation;
pub use runtime::CapabilityCommitError;
pub use runtime::CapabilityCommitReport;
pub use runtime::CapabilityDisposer;
pub use runtime::CapabilityLifecycle;
pub use runtime::CapabilityPayloadError;
pub use runtime::CapabilityRuntime;
pub use runtime::CapabilityScope;
pub use runtime::CapabilitySnapshot;
pub use runtime::CapabilityTransaction;
pub use runtime::GenerationSnapshot;
pub use runtime::RuntimeSnapshot;
pub use scope::ScopeGraph;
pub use scope::ScopeGraphError;
pub use scope::ScopeKind;
