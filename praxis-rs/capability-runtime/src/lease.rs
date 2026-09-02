use crate::CapabilityId;
use crate::CapabilityLifecycle;
use crate::GenerationId;
use crate::runtime::GenerationRecord;
use std::fmt;
use std::ops::Deref;
use std::sync::Arc;

pub struct CapabilityLease {
    capability: CapabilityId,
    generation: Arc<GenerationRecord>,
}

impl CapabilityLease {
    pub(crate) fn new(capability: CapabilityId, generation: Arc<GenerationRecord>) -> Self {
        Self {
            capability,
            generation,
        }
    }

    pub fn capability_id(&self) -> &CapabilityId {
        &self.capability
    }

    pub fn generation_id(&self) -> GenerationId {
        self.generation.id()
    }

    pub fn lifecycle(&self) -> CapabilityLifecycle {
        self.generation.lifecycle()
    }
}

impl Clone for CapabilityLease {
    fn clone(&self) -> Self {
        assert!(
            self.generation.retain_lease(),
            "cannot clone a retired capability lease"
        );
        Self {
            capability: self.capability.clone(),
            generation: Arc::clone(&self.generation),
        }
    }
}

impl fmt::Debug for CapabilityLease {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CapabilityLease")
            .field("capability", &self.capability)
            .field("generation", &self.generation.id())
            .field("lifecycle", &self.lifecycle())
            .finish()
    }
}

impl Drop for CapabilityLease {
    fn drop(&mut self) {
        self.generation.release_lease();
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
