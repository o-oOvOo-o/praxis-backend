//! Content-addressed artifacts shared by agent and workflow runtimes.

use anyhow::Context;
use anyhow::Result;
use serde::Deserialize;
use serde::Serialize;
use sha2::Digest;
use sha2::Sha256;
use std::fs;
use std::io::Read;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct ArtifactRef {
    pub digest: String,
    pub bytes: u64,
    pub media_type: String,
}

#[derive(Clone, Debug)]
pub struct ArtifactStore {
    root: PathBuf,
    max_bytes: u64,
}

impl ArtifactStore {
    pub fn new(root: impl Into<PathBuf>, max_bytes: u64) -> Self {
        Self {
            root: root.into(),
            max_bytes,
        }
    }

    pub fn put(&self, media_type: impl Into<String>, bytes: &[u8]) -> Result<ArtifactRef> {
        let size = u64::try_from(bytes.len()).context("artifact size exceeds u64")?;
        anyhow::ensure!(
            size <= self.max_bytes,
            "artifact exceeds {} byte limit",
            self.max_bytes
        );
        let digest = format!("{:x}", Sha256::digest(bytes));
        let reference = ArtifactRef {
            digest,
            bytes: size,
            media_type: media_type.into(),
        };
        let path = self.path(&reference.digest)?;
        if path.exists() {
            if self.get(&reference).is_ok_and(|stored| stored == bytes) {
                return Ok(reference);
            }
            fs::remove_file(&path)
                .with_context(|| format!("remove corrupt artifact {}", path.display()))?;
        }
        let parent = path.parent().context("artifact path has no parent")?;
        fs::create_dir_all(parent)?;
        let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
        {
            let mut encoder = zstd::stream::write::Encoder::new(temporary.as_file_mut(), 3)?;
            encoder.write_all(bytes)?;
            encoder.finish()?;
        }
        match temporary.persist_noclobber(&path) {
            Ok(_) => Ok(reference),
            Err(error)
                if path.exists() && self.get(&reference).is_ok_and(|stored| stored == bytes) =>
            {
                drop(error);
                Ok(reference)
            }
            Err(error) => Err(error.error.into()),
        }
    }

    pub fn get(&self, reference: &ArtifactRef) -> Result<Vec<u8>> {
        anyhow::ensure!(
            reference.bytes <= self.max_bytes,
            "artifact exceeds read limit"
        );
        let path = self.path(&reference.digest)?;
        let file =
            fs::File::open(&path).with_context(|| format!("open artifact {}", path.display()))?;
        let mut decoder = zstd::stream::read::Decoder::new(file)?;
        let mut bytes = Vec::new();
        decoder
            .by_ref()
            .take(reference.bytes.saturating_add(1))
            .read_to_end(&mut bytes)?;
        anyhow::ensure!(
            u64::try_from(bytes.len())? == reference.bytes,
            "artifact size mismatch"
        );
        anyhow::ensure!(
            format!("{:x}", Sha256::digest(&bytes)) == reference.digest,
            "artifact digest mismatch"
        );
        Ok(bytes)
    }

    pub fn contains(&self, reference: &ArtifactRef) -> bool {
        self.get(reference).is_ok()
    }

    fn path(&self, digest: &str) -> Result<PathBuf> {
        validate_digest(digest)?;
        Ok(self.root.join(&digest[..2]).join(format!("{digest}.zst")))
    }
}

fn validate_digest(digest: &str) -> Result<()> {
    anyhow::ensure!(
        digest.len() == 64 && digest.bytes().all(|byte| byte.is_ascii_hexdigit()),
        "invalid artifact digest"
    );
    Ok(())
}

pub fn default_store(root: &Path) -> ArtifactStore {
    ArtifactStore::new(root.join("artifacts"), 4 * 1024 * 1024 * 1024)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_address_round_trip_is_idempotent() {
        let root = tempfile::tempdir().expect("tempdir");
        let store = ArtifactStore::new(root.path(), 1024);
        let first = store.put("text/plain", b"praxis").expect("put");
        let second = store.put("text/plain", b"praxis").expect("put again");
        assert_eq!(first, second);
        assert_eq!(store.get(&first).expect("get"), b"praxis");
    }

    #[test]
    fn traversal_and_oversized_artifacts_are_rejected() {
        let root = tempfile::tempdir().expect("tempdir");
        let store = ArtifactStore::new(root.path(), 3);
        assert!(store.put("text/plain", b"large").is_err());
        let invalid = ArtifactRef {
            digest: "../outside".into(),
            bytes: 1,
            media_type: "x".into(),
        };
        assert!(store.get(&invalid).is_err());
    }
}
