use super::catalog::LocalModelEntry;
use super::catalog::NativeLocalModelConfig;
use super::catalog::TOKENIZER_FILE_NAME;
use super::catalog::discover_local_models_from_runtime_config_uncached;
use super::catalog::is_supported_model_file;
use once_cell::sync::Lazy;
use sha1::Digest;
use sha1::Sha1;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::UNIX_EPOCH;
use walkdir::WalkDir;

static LOCAL_MODEL_CATALOG_CACHE: Lazy<LocalModelCatalogCache> =
    Lazy::new(LocalModelCatalogCache::default);

const MAX_CACHED_LOCAL_MODEL_CATALOGS: usize = 8;

#[derive(Debug, Default)]
struct LocalModelCatalogCache {
    snapshots: Mutex<BTreeMap<[u8; 20], LocalModelCatalogSnapshot>>,
}

#[derive(Debug, Clone)]
struct LocalModelCatalogSnapshot {
    input_signature: [u8; 20],
    entries: Vec<LocalModelEntry>,
}

pub(super) fn discover_local_models_cached(
    config: &NativeLocalModelConfig,
) -> Vec<LocalModelEntry> {
    let input_signature = discovery_input_signature(config);
    LOCAL_MODEL_CATALOG_CACHE.get_or_discover(config, input_signature, || {
        discover_local_models_from_runtime_config_uncached(config)
    })
}

impl LocalModelCatalogCache {
    fn get_or_discover(
        &self,
        config: &NativeLocalModelConfig,
        input_signature: [u8; 20],
        discover: impl FnOnce() -> Vec<LocalModelEntry>,
    ) -> Vec<LocalModelEntry> {
        let config_signature = config_signature(config);
        let mut snapshots = self
            .snapshots
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(snapshot) = snapshots.get(&config_signature)
            && snapshot.input_signature == input_signature
        {
            return snapshot.entries.clone();
        }

        let entries = discover();
        if snapshots.len() >= MAX_CACHED_LOCAL_MODEL_CATALOGS
            && !snapshots.contains_key(&config_signature)
        {
            snapshots.pop_first();
        }
        snapshots.insert(
            config_signature,
            LocalModelCatalogSnapshot {
                input_signature,
                entries: entries.clone(),
            },
        );
        entries
    }
}

fn config_signature(config: &NativeLocalModelConfig) -> [u8; 20] {
    let mut hasher = Sha1::new();
    hasher.update(config.local_models.scan_max_depth.to_le_bytes());
    for path in &config.local_models.paths {
        hash_path(&mut hasher, path.as_path());
    }
    for (host_id, host) in &config.local_model_hosts {
        hasher.update(host_id.as_bytes());
        hasher.update([0]);
        let host_json = serde_json::to_string(host).unwrap_or_else(|_| format!("{host:?}"));
        hasher.update(host_json.as_bytes());
        hasher.update([0]);
    }
    hasher.finalize().into()
}

fn discovery_input_signature(config: &NativeLocalModelConfig) -> [u8; 20] {
    let mut inputs = Vec::<(u8, PathBuf)>::new();
    let scan_max_depth = config.local_models.scan_max_depth.max(1);
    for root in &config.local_models.paths {
        for entry in WalkDir::new(root)
            .follow_links(false)
            .max_depth(scan_max_depth.saturating_add(1))
            .into_iter()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_type().is_file())
        {
            let path = entry.path();
            if is_supported_model_file(path) {
                inputs.push((0, path.to_path_buf()));
            } else if path
                .file_name()
                .is_some_and(|name| name == TOKENIZER_FILE_NAME)
            {
                inputs.push((1, path.to_path_buf()));
            }
        }
    }
    for host in config.local_model_hosts.values() {
        if let Some(model_path) = &host.model_path {
            inputs.push((2, model_path.to_path_buf()));
        }
        if let Some(tokenizer_path) = &host.tokenizer_path {
            inputs.push((3, tokenizer_path.to_path_buf()));
        }
    }
    inputs.sort();
    inputs.dedup();

    let mut hasher = Sha1::new();
    for (kind, path) in inputs {
        hasher.update([kind]);
        hash_path(&mut hasher, &path);
        hash_metadata(&mut hasher, &path);
    }
    hasher.finalize().into()
}

fn hash_path(hasher: &mut Sha1, path: &Path) {
    hasher.update(path.to_string_lossy().as_bytes());
    hasher.update([0]);
}

fn hash_metadata(hasher: &mut Sha1, path: &Path) {
    let Ok(metadata) = fs::metadata(path) else {
        hasher.update([0]);
        return;
    };
    hasher.update([1]);
    hasher.update(metadata.len().to_le_bytes());
    let Ok(modified) = metadata.modified() else {
        hasher.update([0]);
        return;
    };
    match modified.duration_since(UNIX_EPOCH) {
        Ok(elapsed) => {
            hasher.update([1]);
            hasher.update(elapsed.as_secs().to_le_bytes());
            hasher.update(elapsed.subsec_nanos().to_le_bytes());
        }
        Err(error) => {
            hasher.update([2]);
            hasher.update(error.duration().as_secs().to_le_bytes());
            hasher.update(error.duration().subsec_nanos().to_le_bytes());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::LocalModelsConfig;
    use praxis_utils_absolute_path::AbsolutePathBuf;
    use std::fs;
    use std::sync::Arc;
    use std::sync::Barrier;
    use std::sync::atomic::AtomicUsize;
    use std::sync::atomic::Ordering;
    use std::thread;
    use std::time::Duration;
    use tempfile::tempdir;

    fn config_for_root(root: &Path) -> NativeLocalModelConfig {
        NativeLocalModelConfig {
            local_models: LocalModelsConfig {
                paths: vec![AbsolutePathBuf::from_absolute_path(root).unwrap()],
                scan_max_depth: 2,
            },
            ..Default::default()
        }
    }

    #[test]
    fn unchanged_inputs_reuse_the_catalog_without_a_ttl_rescan() {
        let cache = LocalModelCatalogCache::default();
        let config = NativeLocalModelConfig::default();
        let calls = AtomicUsize::new(0);

        for _ in 0..2 {
            cache.get_or_discover(&config, [7; 20], || {
                calls.fetch_add(1, Ordering::Relaxed);
                Vec::new()
            });
        }

        assert_eq!(calls.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn concurrent_misses_are_coalesced_into_one_discovery() {
        let cache = Arc::new(LocalModelCatalogCache::default());
        let barrier = Arc::new(Barrier::new(3));
        let calls = Arc::new(AtomicUsize::new(0));
        let mut workers = Vec::new();

        for _ in 0..2 {
            let cache = Arc::clone(&cache);
            let barrier = Arc::clone(&barrier);
            let calls = Arc::clone(&calls);
            workers.push(thread::spawn(move || {
                barrier.wait();
                cache.get_or_discover(&NativeLocalModelConfig::default(), [9; 20], || {
                    calls.fetch_add(1, Ordering::Relaxed);
                    thread::sleep(Duration::from_millis(25));
                    Vec::new()
                })
            }));
        }

        barrier.wait();
        for worker in workers {
            worker.join().unwrap();
        }
        assert_eq!(calls.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn distinct_configs_do_not_evict_each_other() {
        let cache = LocalModelCatalogCache::default();
        let first = NativeLocalModelConfig::default();
        let mut second = NativeLocalModelConfig::default();
        second.local_models.scan_max_depth = 2;
        let calls = AtomicUsize::new(0);

        for (config, signature) in [(&first, [1; 20]), (&second, [2; 20]), (&first, [1; 20])] {
            cache.get_or_discover(config, signature, || {
                calls.fetch_add(1, Ordering::Relaxed);
                Vec::new()
            });
        }

        assert_eq!(calls.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn input_signature_tracks_model_and_tokenizer_changes() {
        let root = tempdir().unwrap();
        let config = config_for_root(root.path());
        let empty = discovery_input_signature(&config);

        fs::write(root.path().join("model.gguf"), b"GGUF").unwrap();
        let model_added = discovery_input_signature(&config);
        assert_ne!(model_added, empty);

        fs::write(root.path().join("model.gguf"), b"GGUF-expanded").unwrap();
        let model_changed = discovery_input_signature(&config);
        assert_ne!(model_changed, model_added);

        fs::write(root.path().join(TOKENIZER_FILE_NAME), b"{}").unwrap();
        assert_ne!(discovery_input_signature(&config), model_changed);
    }
}
