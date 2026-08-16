use crate::CapabilityId;
use crate::CapabilityLifecycle;
use crate::GenerationId;
use crate::runtime::RuntimeInner;
use std::fmt;
use std::ops::Deref;
use std::sync::Arc;

pub struct CapabilityLease {
    runtime: Arc<RuntimeInner>,
    capability: CapabilityId,
    generation: GenerationId,
}

impl CapabilityLease {
    pub(crate) fn new(
        runtime: Arc<RuntimeInner>,
        capability: CapabilityId,
        generation: GenerationId,
    ) -> Self {
        Self {
            runtime,
            capability,
            generation,
        }
    }

    pub fn capability_id(&self) -> &CapabilityId {
        &self.capability
    }

    pub fn generation_id(&self) -> GenerationId {
        self.generation
    }

    pub fn lifecycle(&self) -> CapabilityLifecycle {
        self.runtime.lease_lifecycle(self.generation)
    }
}

impl Clone for CapabilityLease {
    fn clone(&self) -> Self {
        assert!(
            self.runtime.clone_lease(self.generation),
            "cannot clone a retired capability lease"
        );
        Self {
            runtime: Arc::clone(&self.runtime),
            capability: self.capability.clone(),
            generation: self.generation,
        }
    }
}

impl fmt::Debug for CapabilityLease {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CapabilityLease")
            .field("capability", &self.capability)
            .field("generation", &self.generation)
            .field("lifecycle", &self.lifecycle())
            .finish()
    }
}

impl Drop for CapabilityLease {
    fn drop(&mut self) {
        self.runtime.release_lease(self.generation);
    }
}

/// A typed value borrowed from one published capability generation.
///
/// The embedded lease keeps the generation alive until the final clone of the
/// carrier is dropped, including while that generation is quiescing.
pub struct TypedCapability<T> {
    value: Arc<T>,
    lease: CapabilityLease,
}

impl<T> TypedCapability<T> {
    pub(crate) fn new(value: Arc<T>, lease: CapabilityLease) -> Self {
        Self { value, lease }
    }

    pub fn value(&self) -> &T {
        self.value.as_ref()
    }

    pub fn lease(&self) -> &CapabilityLease {
        &self.lease
    }
}

impl<T> Clone for TypedCapability<T> {
    fn clone(&self) -> Self {
        Self {
            value: Arc::clone(&self.value),
            lease: self.lease.clone(),
        }
    }
}

impl<T> Deref for TypedCapability<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.value()
    }
}

impl<T> fmt::Debug for TypedCapability<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TypedCapability")
            .field("lease", &self.lease)
            .finish_non_exhaustive()
    }
}
