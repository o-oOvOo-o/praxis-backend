// TODO(ccunningham): Add a coherent live-turn sampling snapshot once core can
// represent sampling boundaries instead of relying on persisted mid-turn items.
//
// Two likely future variants:
// - `TruncateToLastSamplingBoundary` for callers that want a coherent fork from
//   the last stable model boundary without synthesizing an interrupt.
// - `WaitUntilNextSamplingBoundary` (or similar) for callers that prefer to
//   fork after the next sampling boundary rather than interrupting immediately.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThreadForkSnapshot {
    /// Fork a committed prefix ending strictly before the nth user message.
    ///
    /// When `n` is within range, this cuts before that 0-based user-message
    /// boundary. When `n` is out of range and the source thread is currently
    /// mid-turn, this instead cuts before the active turn's opening boundary
    /// so the fork drops the unfinished turn suffix. When `n` is out of range
    /// and the source thread is already at a turn boundary, this returns the
    /// full committed history unchanged.
    TruncateBeforeNthUserMessage(usize),

    /// Fork the current persisted history and close an incomplete copied turn.
    ///
    /// If the persisted snapshot ends mid-turn, this appends the same
    /// `<turn_aborted>` boundary used for interrupted history. The source
    /// thread remains loaded and continues running. If the snapshot is already
    /// at a turn boundary, this returns the persisted history unchanged.
    CurrentPersistedWithClosedTurn,
}
