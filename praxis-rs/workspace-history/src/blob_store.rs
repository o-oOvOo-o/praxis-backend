use std::fs;
use std::io::Read;
use std::io::Write;
use std::path::PathBuf;

use anyhow::Context;
use anyhow::Result;
use sha2::Digest;
use sha2::Sha256;

pub(crate) struct BlobStore {
    root: PathBuf,
}

impl BlobStore {
    pub(crate) fn new(root: PathBuf) -> Self {
        Self { root }
    }

    pub(crate) fn put(&self, bytes: &[u8]) -> Result<String> {
        let hash = format!("{:x}", Sha256::digest(bytes));
        let path = self.path(&hash)?;
        if path.exists() {
            if self
                .get_limited(&hash, u64::try_from(bytes.len()).unwrap_or(u64::MAX))
                .is_ok_and(|existing| existing == bytes)
            {
                return Ok(hash);
            }
            fs::remove_file(&path)?;
        }

        let parent = path.parent().context("blob path has no parent")?;
        fs::create_dir_all(parent)?;
        let mut temp = tempfile::NamedTempFile::new_in(parent)?;
        {
            let mut encoder = zstd::stream::write::Encoder::new(temp.as_file_mut(), 3)?;
            encoder.write_all(bytes)?;
            encoder.finish()?;
        }
        match temp.persist_noclobber(&path) {
            Ok(_) => {}
            Err(error) if path.exists() => drop(error),
            Err(error) => return Err(error.error.into()),
        }
        Ok(hash)
    }

    pub(crate) fn get_limited(&self, hash: &str, expected_bytes: u64) -> Result<Vec<u8>> {
        let path = self.path(hash)?;
        let file = fs::File::open(&path)
            .with_context(|| format!("open workspace history blob {}", path.display()))?;
        let mut decoder = zstd::stream::read::Decoder::new(file)?;
        let mut bytes = Vec::new();
        decoder
            .by_ref()
            .take(expected_bytes.saturating_add(1))
            .read_to_end(&mut bytes)?;
        if u64::try_from(bytes.len()).unwrap_or(u64::MAX) != expected_bytes {
            anyhow::bail!("workspace history blob size does not match manifest");
        }
        let actual_hash = format!("{:x}", Sha256::digest(&bytes));
        if actual_hash != hash {
            anyhow::bail!("workspace history blob content hash mismatch");
        }
        Ok(bytes)
    }

    fn path(&self, hash: &str) -> Result<PathBuf> {
        if hash.len() != 64 || !hash.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            anyhow::bail!("invalid workspace history blob hash");
        }
        Ok(self.root.join(&hash[..2]).join(format!("{hash}.zst")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let path = store.path(&hash).expect("blob path");
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
