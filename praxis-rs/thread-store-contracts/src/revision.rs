use crate::CanonicalEncode;
use crate::CanonicalHasher;
use crate::Digest;
use crate::ThreadId;
use serde::Deserialize;
use serde::Serialize;

#[derive(
    Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize,
)]
#[serde(transparent)]
pub struct ThreadRevision(u64);

impl ThreadRevision {
    pub const ZERO: Self = Self(0);

    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    pub const fn get(self) -> u64 {
        self.0
    }

    pub const fn checked_next(self) -> Option<Self> {
        match self.0.checked_add(1) {
            Some(value) => Some(Self(value)),
            None => None,
        }
    }

    pub const fn checked_advance(self, count: u64) -> Option<Self> {
        match self.0.checked_add(count) {
            Some(value) => Some(Self(value)),
            None => None,
        }
    }
}

impl CanonicalEncode for ThreadRevision {
    fn encode_canonical(&self, hasher: &mut CanonicalHasher) {
        hasher.u64(self.0);
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ThreadHead {
    pub revision: ThreadRevision,
    pub record_digest: Digest,
}

impl ThreadHead {
    pub const EMPTY: Self = Self {
        revision: ThreadRevision::ZERO,
        record_digest: Digest::ZERO,
    };
}

impl CanonicalEncode for ThreadHead {
    fn encode_canonical(&self, hasher: &mut CanonicalHasher) {
        self.revision.encode_canonical(hasher);
        self.record_digest.encode_canonical(hasher);
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ThreadRevisionRef {
    pub thread_id: ThreadId,
    pub revision: ThreadRevision,
}

impl CanonicalEncode for ThreadRevisionRef {
    fn encode_canonical(&self, hasher: &mut CanonicalHasher) {
        self.thread_id.encode_canonical(hasher);
        self.revision.encode_canonical(hasher);
    }
}
