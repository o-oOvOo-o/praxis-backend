use anyhow::Result;
use praxis_artifacts::ArtifactRef;
use praxis_artifacts::ArtifactStore;
use std::path::PathBuf;

const MEDIA_TYPE: &str = "application/vnd.praxis.workspace-history";

pub(crate) struct BlobStore {
    artifacts: ArtifactStore,
}

impl BlobStore {
    pub(crate) fn new(root: PathBuf) -> Self {
        Self {
            artifacts: ArtifactStore::new(root, u64::MAX),
        }
    }

    pub(crate) fn put(&self, bytes: &[u8]) -> Result<String> {
        Ok(self.artifacts.put(MEDIA_TYPE, bytes)?.digest)
    }

    pub(crate) fn get_limited(&self, hash: &str, expected_bytes: u64) -> Result<Vec<u8>> {
        self.artifacts.get(&ArtifactRef {
            digest: hash.to_owned(),
            bytes: expected_bytes,
            media_type: MEDIA_TYPE.into(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn invalid_blob_hash_is_rejected_without_panicking() {
        let store = BlobStore::new(PathBuf::from("unused"));
        assert!(store.get_limited("x", 1).is_err());
        assert!(store.get_limited("../outside", 1).is_err());
    }

    #[test]
    fn put_repairs_an_existing_corrupt_blob() {
        let temp = tempfile::tempdir().expect("tempdir");
        let store = BlobStore::new(temp.path().to_path_buf());
        let bytes = b"checkpoint data";
        let hash = store.put(bytes).expect("initial put");
        let path = temp.path().join(&hash[..2]).join(format!("{hash}.zst"));
        fs::write(&path, b"not zstd").expect("corrupt blob");

        assert_eq!(store.put(bytes).expect("repair blob"), hash);
        assert_eq!(
            store
                .get_limited(
                    &hash,
                    u64::try_from(bytes.len()).expect("test payload length fits u64"),
                )
                .expect("read repaired blob"),
            bytes
        );
    }
}
