use crate::JournalConfig;
use crate::JournalError;
use crate::journal::validate_config;
use crate::recovery::recover_snapshot_consume;
use praxis_thread_store_contracts::ThreadEventEnvelope;
use praxis_thread_store_contracts::ThreadId;

/// Validate and fold an immutable committed prefix in one decode pass.
pub fn fold_snapshot<S>(
    config: JournalConfig,
    thread_id: ThreadId,
    state: S,
    mut fold: impl FnMut(&mut S, &ThreadEventEnvelope),
) -> Result<S, JournalError> {
    consume_snapshot(config, thread_id, state, |state, event| fold(state, &event))
}

/// Validate and consume an immutable committed prefix in one decode pass.
pub fn consume_snapshot<S>(
    config: JournalConfig,
    thread_id: ThreadId,
    state: S,
    consume: impl FnMut(&mut S, ThreadEventEnvelope),
) -> Result<S, JournalError> {
    validate_config(&config)?;
    recover_snapshot_consume(
        &config,
        thread_id,
        &config
            .root
            .join("threads")
            .join(thread_id.to_string())
            .join("journal")
            .join("segments"),
        state,
        consume,
    )
}

#[cfg(test)]
mod boundary_tests {
    #[test]
    fn snapshot_surface_cannot_build_a_replayable_frame_index() {
        let source = include_str!("snapshot.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("snapshot production source");
        assert!(!source.contains("struct ThreadJournalSnapshot"));
        assert!(!source.contains("recover_snapshot("));
        assert!(!source.contains("project_range"));
        assert!(!source.contains("read_all"));
        assert!(source.contains("recover_snapshot_consume"));
    }
}
