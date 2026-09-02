use lru::LruCache;
use sha1::Digest;
use sha1::Sha1;
use std::borrow::Borrow;
use std::convert::Infallible;
use std::hash::Hash;
use std::num::NonZeroUsize;
use std::panic::AssertUnwindSafe;
use std::sync::Arc;
use std::sync::Condvar;
use std::sync::Mutex;
use std::sync::MutexGuard;

/// A bounded, synchronous memoization table.
///
/// Values are cloned out of the table so no cache lock escapes into caller code. A miss installs a
/// per-key computation slot before invoking the producer, allowing unrelated keys to progress and
/// ensuring concurrent requests for the same resident key share one computation.
pub struct MemoCache<K, V> {
    entries: Mutex<LruCache<K, Arc<Slot<V>>>>,
}

/// A small, thread-safe LRU map for already-computed snapshots.
///
/// Unlike [`MemoCache`], this type does not coordinate value production. Callers use `with_mut`
/// when insertion must be atomic with a final duplicate check.
pub struct LruMap<K, V> {
    entries: Mutex<LruCache<K, V>>,
}

impl<K, V> LruMap<K, V>
where
    K: Eq + Hash,
{
    pub fn new(capacity: NonZeroUsize) -> Self {
        Self {
            entries: Mutex::new(LruCache::new(capacity)),
        }
    }

    pub fn get<Q>(&self, key: &Q) -> Option<V>
    where
        K: Borrow<Q>,
        Q: Eq + Hash + ?Sized,
        V: Clone,
    {
        lock(&self.entries).get(key).cloned()
    }

    pub fn clear(&self) {
        lock(&self.entries).clear();
    }

    pub fn len(&self) -> usize {
        lock(&self.entries).len()
    }

    pub fn with_mut<R>(&self, operation: impl FnOnce(&mut LruCache<K, V>) -> R) -> R {
        operation(&mut lock(&self.entries))
    }
}

impl<K, V> MemoCache<K, V>
where
    K: Eq + Hash,
    V: Clone,
{
    pub fn new(capacity: NonZeroUsize) -> Self {
        Self {
            entries: Mutex::new(LruCache::new(capacity)),
        }
    }

    pub fn get_or_compute<F>(&self, key: K, producer: F) -> V
    where
        F: FnOnce() -> V,
    {
        match self.try_get_or_compute(key, || Ok::<V, Infallible>(producer())) {
            Ok(value) => value,
            Err(never) => match never {},
        }
    }

    pub fn clear(&self) {
        lock(&self.entries).clear();
    }

    pub fn try_get_or_compute<F, E>(&self, key: K, producer: F) -> Result<V, E>
    where
        F: FnOnce() -> Result<V, E>,
    {
        let slot = {
            let mut entries = lock(&self.entries);
            if let Some(slot) = entries.get(&key) {
                Arc::clone(slot)
            } else {
                let slot = Arc::new(Slot::new());
                entries.put(key, Arc::clone(&slot));
                slot
            }
        };

        slot.get_or_compute(producer)
    }
}

struct Slot<V> {
    state: Mutex<SlotState<V>>,
    changed: Condvar,
}

enum SlotState<V> {
    Empty,
    Computing,
    Ready(V),
}

impl<V: Clone> Slot<V> {
    fn new() -> Self {
        Self {
            state: Mutex::new(SlotState::Empty),
            changed: Condvar::new(),
        }
    }

    fn get_or_compute<F, E>(&self, producer: F) -> Result<V, E>
    where
        F: FnOnce() -> Result<V, E>,
    {
        let mut producer = Some(producer);

        loop {
            let mut state = lock(&self.state);
            match &*state {
                SlotState::Ready(value) => return Ok(value.clone()),
                SlotState::Computing => {
                    state = wait(&self.changed, state);
                    drop(state);
                }
                SlotState::Empty => {
                    *state = SlotState::Computing;
                    drop(state);

                    let run = producer
                        .take()
                        .expect("a cache caller can own at most one computation");
                    match std::panic::catch_unwind(AssertUnwindSafe(run)) {
                        Ok(Ok(value)) => {
                            let mut state = lock(&self.state);
                            *state = SlotState::Ready(value.clone());
                            self.changed.notify_all();
                            return Ok(value);
                        }
                        Ok(Err(error)) => {
                            let mut state = lock(&self.state);
                            *state = SlotState::Empty;
                            self.changed.notify_all();
                            return Err(error);
                        }
                        Err(payload) => {
                            let mut state = lock(&self.state);
                            *state = SlotState::Empty;
                            self.changed.notify_all();
                            drop(state);
                            std::panic::resume_unwind(payload);
                        }
                    }
                }
            }
        }
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn wait<'a, T>(condition: &Condvar, guard: MutexGuard<'a, T>) -> MutexGuard<'a, T> {
    condition
        .wait(guard)
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Computes the stable 20-byte SHA-1 fingerprint used by content-addressed cache keys.
pub fn sha1_fingerprint(bytes: &[u8]) -> [u8; 20] {
    let digest = Sha1::digest(bytes);
    let mut fingerprint = [0; 20];
    fingerprint.copy_from_slice(&digest);
    fingerprint
}
