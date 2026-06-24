//! Praxis-owned approval control plane.

pub mod controller;
pub mod decision;
pub mod handle;
pub mod live;
pub mod outcome;
pub mod request;
pub mod resolve;
pub mod state;
pub mod store;
pub mod sync;
pub mod tool_safety;

pub use controller::PermissionController;
pub use decision::ApprovalDecision;
pub use handle::PermissionHandle;
pub use live::LivePermissions;
pub use outcome::ApprovalOutcome;
pub use request::ApprovalRequest;
pub use resolve::PermissionConflict;
pub use resolve::PermissionOverride;
pub use resolve::PermissionResolver;
pub use state::ApprovalCacheScope;
pub use state::PermissionPreset;
pub use state::PermissionStateSource;
pub use state::ResolvedTurnPermissions;
pub use state::ThreadPermissionState;
pub use state::ThreadPermissions;
pub use state::TurnPermissions;
pub use store::ApprovalCache;
pub use store::ApprovalRecord;
pub use store::PendingApproval;
pub use store::PendingApprovalStore;
pub use sync::PermissionSync;
