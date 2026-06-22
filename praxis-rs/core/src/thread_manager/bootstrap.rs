use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;

use praxis_protocol::ThreadId;
use praxis_protocol::protocol::Op;
use tokio::runtime::Handle;
use tokio::runtime::RuntimeFlavor;
use tokio::sync::broadcast;
use tracing::warn;

use crate::SkillsManager;
use crate::file_watcher::FileWatcher;
use crate::skills_watcher::SkillsWatcher;
use crate::skills_watcher::SkillsWatcherEvent;

/// Test-only override for enabling thread-manager behaviors used by integration
/// tests.
///
/// In production builds this value should remain at its default (`false`) and
/// must not be toggled.
static FORCE_TEST_THREAD_MANAGER_BEHAVIOR: AtomicBool = AtomicBool::new(false);

type CapturedOps = Vec<(ThreadId, Op)>;
pub(super) type SharedCapturedOps = Arc<Mutex<CapturedOps>>;

pub(crate) fn set_thread_manager_test_mode_for_tests(enabled: bool) {
    FORCE_TEST_THREAD_MANAGER_BEHAVIOR.store(enabled, Ordering::Relaxed);
}

pub(super) fn should_use_test_thread_manager_behavior() -> bool {
    FORCE_TEST_THREAD_MANAGER_BEHAVIOR.load(Ordering::Relaxed)
}

pub(super) struct TempPraxisHomeGuard {
    pub(super) path: PathBuf,
}

impl Drop for TempPraxisHomeGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

pub(super) fn build_skills_watcher(skills_manager: Arc<SkillsManager>) -> Arc<SkillsWatcher> {
    if should_use_test_thread_manager_behavior()
        && let Ok(handle) = Handle::try_current()
        && handle.runtime_flavor() == RuntimeFlavor::CurrentThread
    {
        // The real watcher spins background tasks that can starve the
        // current-thread test runtime and cause event waits to time out.
        warn!("using noop skills watcher under current-thread test runtime");
        return Arc::new(SkillsWatcher::noop());
    }

    let file_watcher = match FileWatcher::new() {
        Ok(file_watcher) => Arc::new(file_watcher),
        Err(err) => {
            warn!("failed to initialize file watcher: {err}");
            Arc::new(FileWatcher::noop())
        }
    };
    let skills_watcher = Arc::new(SkillsWatcher::new(&file_watcher));

    let mut rx = skills_watcher.subscribe();
    let skills_manager = Arc::clone(&skills_manager);
    if let Ok(handle) = Handle::try_current() {
        handle.spawn(async move {
            loop {
                match rx.recv().await {
                    Ok(SkillsWatcherEvent::SkillsChanged { .. }) => {
                        skills_manager.clear_cache();
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                }
            }
        });
    } else {
        warn!("skills watcher listener skipped: no Tokio runtime available");
    }

    skills_watcher
}
