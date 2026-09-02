//! Host-neutral contracts for the Praxis native ThreadStore.

#![forbid(unsafe_code)]

mod canonical;
mod command;
mod event;
mod ids;
mod projection;
mod receipt;
mod revision;
mod turn_lifecycle;

pub use canonical::CanonicalEncode;
pub use canonical::CanonicalHasher;
pub use canonical::Digest;
pub use command::ThreadCommand;
pub use command::ThreadCommandEnvelope;
pub use command::ThreadCommandHeader;
pub use command::ThreadResumeConfig;
pub use event::AgentEventRoute;
pub use event::ContentRef;
pub use event::NewThreadEvent;
pub use event::ThreadActor;
pub use event::ThreadEventBody;
pub use event::ThreadEventEnvelope;
pub use event::TranscriptItemKind;
pub use event::TurnAbortReason;
pub use ids::BatchId;
pub use ids::CommandId;
pub use ids::EventId;
pub use ids::ItemId;
pub use ids::ReceiptId;
pub use ids::ThreadId;
pub use ids::TurnId;
pub use projection::DeterminismClass;
pub use projection::PluginCapability;
pub use projection::ProjectionCheckpoint;
pub use projection::ProjectionDescriptor;
pub use projection::ProjectionId;
pub use projection::ProjectionPriority;
pub use projection::ReadConsistency;
pub use projection::RebuildBehavior;
pub use projection::SchemaRange;
pub use projection::ThreadStorePluginDescriptor;
pub use receipt::AchievedDurability;
pub use receipt::CommittedEventRef;
pub use receipt::ReceiptDiagnostic;
pub use receipt::ReceiptStatus;
pub use receipt::ReceiptValidationError;
pub use receipt::ThreadCommandReceipt;
pub use revision::ThreadHead;
pub use revision::ThreadRevision;
pub use revision::ThreadRevisionRef;
pub use turn_lifecycle::TurnLifecycle;
pub use turn_lifecycle::TurnLifecycleError;
pub use turn_lifecycle::TurnTransition;
pub use turn_lifecycle::TurnTransitionOutcome;

pub const THREAD_STORE_SCHEMA_VERSION: u32 = 1;
