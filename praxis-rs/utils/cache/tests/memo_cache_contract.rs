use praxis_utils_cache::MemoCache;
use praxis_utils_cache::sha1_fingerprint;
use std::num::NonZeroUsize;
use std::sync::Arc;
use std::sync::Barrier;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::thread;
use std::time::Duration;

fn capacity(value: usize) -> NonZeroUsize {
    NonZeroUsize::new(value).expect("test capacity must be non-zero")
}

#[test]
fn least_recently_used_value_is_recomputed_after_eviction() {
    let cache = MemoCache::new(capacity(2));
    let computations = AtomicUsize::new(0);

    assert_eq!(cache.get_or_compute("a", || 10), 10);
    assert_eq!(cache.get_or_compute("b", || 20), 20);
    assert_eq!(cache.get_or_compute("a", || 99), 10);
    assert_eq!(cache.get_or_compute("c", || 30), 30);
    assert_eq!(
        cache.get_or_compute("b", || {
            computations.fetch_add(1, Ordering::Relaxed);
            21
        }),
        21
    );
    assert_eq!(computations.load(Ordering::Relaxed), 1);
}

#[test]
fn failed_computation_does_not_poison_or_populate_the_key() {
    let cache = MemoCache::<&str, usize>::new(capacity(1));

    let failure = cache.try_get_or_compute("image", || Err::<usize, _>("decode failed"));
    assert_eq!(failure, Err("decode failed"));
    assert_eq!(
        cache.try_get_or_compute("image", || Ok::<_, &str>(7)),
        Ok(7)
    );
}

#[test]
fn clear_discards_all_resident_values() {
    let cache = MemoCache::new(capacity(2));

    assert_eq!(cache.get_or_compute("image", || 7), 7);
    cache.clear();
    assert_eq!(cache.get_or_compute("image", || 9), 9);
}

#[test]
fn panicking_computation_releases_the_key_for_retry() {
    let cache = MemoCache::<&str, usize>::new(capacity(1));

    let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        cache.get_or_compute("image", || panic!("decoder panic"));
    }));

    assert!(panic.is_err());
    assert_eq!(cache.get_or_compute("image", || 11), 11);
}

#[test]
fn concurrent_callers_share_one_computation() {
    let cache = Arc::new(MemoCache::new(capacity(4)));
    let ready = Arc::new(Barrier::new(5));
    let computations = Arc::new(AtomicUsize::new(0));
    let mut workers = Vec::new();

    for _ in 0..4 {
        let cache = Arc::clone(&cache);
        let ready = Arc::clone(&ready);
        let computations = Arc::clone(&computations);
        workers.push(thread::spawn(move || {
            ready.wait();
            cache.get_or_compute("shared", || {
                computations.fetch_add(1, Ordering::SeqCst);
                thread::sleep(Duration::from_millis(25));
                42
            })
        }));
    }

    ready.wait();
    for worker in workers {
        assert_eq!(worker.join().expect("worker must finish"), 42);
    }
    assert_eq!(computations.load(Ordering::SeqCst), 1);
}

#[test]
fn content_fingerprint_has_the_standard_sha1_wire_shape() {
    assert_eq!(
        sha1_fingerprint(b"abc"),
        [
            0xa9, 0x99, 0x3e, 0x36, 0x47, 0x06, 0x81, 0x6a, 0xba, 0x3e, 0x25, 0x71, 0x78, 0x50,
            0xc2, 0x6c, 0x9c, 0xd0, 0xd8, 0x9d,
        ]
    );
}
