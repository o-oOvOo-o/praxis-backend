// TODO(ccunningham): Add an explicit non-interrupting live-turn snapshot once
// core can represent sampling boundaries directly instead of relying on
// whichever items happened to be persisted mid-turn.
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

    /// Fork the current persisted history as if the source thread had been
    /// interrupted now.
    ///
    /// If the persisted snapshot ends mid-turn, this appends the same
    /// `<turn_aborted>` marker produced by a real interrupt. If the snapshot is
    /// already at a turn boundary, this returns the current persisted history
    /// unchanged.
    Interrupted,
}
