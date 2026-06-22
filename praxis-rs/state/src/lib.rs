//! SQLite-backed state for rollout metadata.
//!
//! This crate is intentionally small and focused: it extracts rollout metadata
//! from JSONL rollouts and mirrors it into a local SQLite database. Backfill
//! orchestration and rollout scanning live in `praxis-core`.

mod extract;
pub mod log_db;
mod migrations;
mod model;
mod paths;
mod runtime;
pub mod thread_preview;

pub use model::LogEntry;
pub use model::LogQuery;
pub use model::LogRow;
pub use model::Phase2InputSelection;
pub use model::Phase2JobClaimOutcome;
/// Preferred entrypoint: owns configuration and metrics.
pub use runtime::StateRuntime;

/// Low-level storage engine: useful for focused tests.
///
/// Most consumers should prefer [`StateRuntime`].
pub use extract::apply_rollout_item;
pub use extract::rollout_item_affects_thread_metadata;
pub use model::AgentJob;
pub use model::AgentJobCreateParams;
pub use model::AgentJobItem;
pub use model::AgentJobItemCreateParams;
pub use model::AgentJobItemStatus;
pub use model::AgentJobProgress;
pub use model::AgentJobStatus;
pub use model::Anchor;
pub use model::BackfillState;
pub use model::BackfillStats;
pub use model::BackfillStatus;
pub use model::DirectionalThreadSpawnEdgeStatus;
pub use model::ExtractionOutcome;
pub use model::SortKey;
pub use model::Stage1JobClaim;
pub use model::Stage1JobClaimOutcome;
pub use model::Stage1Output;
pub use model::Stage1OutputRef;
pub use model::Stage1StartupClaimParams;
pub use model::ThreadGoal;
pub use model::ThreadGoalStatus;
pub use model::ThreadMetadata;
pub use model::ThreadMetadataBuilder;
pub use model::ThreadSourceKind;
pub use model::ThreadsPage;
pub use runtime::GoalAccountingMode;
pub use runtime::GoalAccountingOutcome;
pub use runtime::GoalUpdate;
pub use runtime::logs_db_filename;
pub use runtime::logs_db_path;
pub use runtime::state_db_filename;
pub use runtime::state_db_path;

/// Environment variable for overriding the Praxis SQLite state database home directory.
///
/// Deliberately separate from CODEX_SQLITE_HOME so Praxis never reads or writes
/// upstream Codex state_*.sqlite files.
pub const SQLITE_HOME_ENV: &str = "PRAXIS_SQLITE_HOME";

pub const LOGS_DB_FILENAME: &str = "logs";
pub const LOGS_DB_VERSION: u32 = 2;
pub const STATE_DB_FILENAME: &str = "state";
pub const STATE_DB_VERSION: u32 = 5;

/// Errors encountered during DB operations. Tags: [stage]
pub const DB_ERROR_METRIC: &str = "praxis.db.error";
/// Metrics on backfill process. Tags: [status]
pub const DB_METRIC_BACKFILL: &str = "praxis.db.backfill";
/// Metrics on backfill duration. Tags: [status]
pub const DB_METRIC_BACKFILL_DURATION_MS: &str = "praxis.db.backfill.duration_ms";
