mod capture;
mod task;
mod timeout_warning;
mod warnings;

#[cfg(test)]
use praxis_git_utils::GhostSnapshotReport;
#[cfg(test)]
use warnings::format_large_untracked_warning;

#[cfg(test)]
#[path = "ghost_snapshot_tests.rs"]
mod tests;
