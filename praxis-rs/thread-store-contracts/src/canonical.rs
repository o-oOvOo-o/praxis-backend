use serde::Deserialize;
use serde::Serialize;
use sha2::Digest as _;
use sha2::Sha256;
use std::fmt;

#[derive(Clone, Copy, Default, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Digest([u8; 32]);

impl Digest {
    pub const ZERO: Self = Self([0; 32]);

    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    pub fn to_hex(self) -> String {
        self.0.iter().map(|byte| format!("{byte:02x}")).collect()
    }
}

impl fmt::Debug for Digest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.to_hex())
    }
}

impl fmt::Display for Digest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.to_hex())
    }
}

pub struct CanonicalHasher {
    hasher: Sha256,
}

impl CanonicalHasher {
    pub fn domain(domain: &str) -> Self {
        let mut value = Self {
            hasher: Sha256::new(),
        };
        value.bytes(domain.as_bytes());
        value
    }

    pub fn bool(&mut self, value: bool) {
        self.u8(u8::from(value));
    }

    pub fn u8(&mut self, value: u8) {
        self.hasher.update([value]);
    }

    pub fn u32(&mut self, value: u32) {
        self.hasher.update(value.to_le_bytes());
    }

    pub fn u64(&mut self, value: u64) {
        self.hasher.update(value.to_le_bytes());
    }

    pub fn i64(&mut self, value: i64) {
        self.hasher.update(value.to_le_bytes());
    }

    pub fn bytes(&mut self, value: &[u8]) {
        self.u64(value.len() as u64);
        self.hasher.update(value);
    }

    pub fn string(&mut self, value: &str) {
        self.bytes(value.as_bytes());
    }

    pub fn digest(&mut self, value: Digest) {
        self.hasher.update(value.as_bytes());
    }

    pub fn optional<T: CanonicalEncode>(&mut self, value: Option<&T>) {
        match value {
            Some(value) => {
                self.bool(true);
                value.encode_canonical(self);
            }
            None => self.bool(false),
        }
    }

    pub fn sequence<T: CanonicalEncode>(&mut self, values: &[T]) {
        self.u64(values.len() as u64);
        for value in values {
            value.encode_canonical(self);
        }
    }

    pub fn finish(self) -> Digest {
        Digest(self.hasher.finalize().into())
    }
}

pub trait CanonicalEncode {
    fn encode_canonical(&self, hasher: &mut CanonicalHasher);

    fn canonical_digest(&self, domain: &str) -> Digest {
        let mut hasher = CanonicalHasher::domain(domain);
        self.encode_canonical(&mut hasher);
        hasher.finish()
    }
}

impl CanonicalEncode for bool {
    fn encode_canonical(&self, hasher: &mut CanonicalHasher) {
        hasher.bool(*self);
    }
}

impl CanonicalEncode for u32 {
    fn encode_canonical(&self, hasher: &mut CanonicalHasher) {
        hasher.u32(*self);
    }
}

impl CanonicalEncode for u64 {
    fn encode_canonical(&self, hasher: &mut CanonicalHasher) {
        hasher.u64(*self);
    }
}

impl CanonicalEncode for i64 {
    fn encode_canonical(&self, hasher: &mut CanonicalHasher) {
        hasher.i64(*self);
    }
}

impl CanonicalEncode for str {
    fn encode_canonical(&self, hasher: &mut CanonicalHasher) {
        hasher.string(self);
    }
}

impl CanonicalEncode for String {
    fn encode_canonical(&self, hasher: &mut CanonicalHasher) {
        self.as_str().encode_canonical(hasher);
    }
}

impl<T: CanonicalEncode> CanonicalEncode for Option<T> {
    fn encode_canonical(&self, hasher: &mut CanonicalHasher) {
        hasher.optional(self.as_ref());
    }
}

impl<T: CanonicalEncode> CanonicalEncode for Vec<T> {
    fn encode_canonical(&self, hasher: &mut CanonicalHasher) {
        hasher.sequence(self);
    }
}

impl CanonicalEncode for Digest {
    fn encode_canonical(&self, hasher: &mut CanonicalHasher) {
        hasher.digest(*self);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_hash_is_length_delimited() {
        let mut first = CanonicalHasher::domain("praxis.test");
        first.bytes(b"ab");
        first.bytes(b"c");
        let mut second = CanonicalHasher::domain("praxis.test");
        second.bytes(b"a");
        second.bytes(b"bc");
        assert_ne!(first.finish(), second.finish());
    }

    #[test]
    fn canonical_hash_is_domain_separated() {
        assert_ne!(
            "same".canonical_digest("praxis.first"),
            "same".canonical_digest("praxis.second")
        );
    }
}
