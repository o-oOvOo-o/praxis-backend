//! Praxis-owned approval control plane.

pub mod live;
pub mod resolve;
pub mod state;
pub mod store;
pub mod sync;

pub use live::LivePermissions;
pub use resolve::PermissionConflict;
pub use resolve::PermissionOverride;
pub use resolve::PermissionResolver;
pub use state::ApprovalCacheScope;
pub use state::PermissionPreset;
pub use state::ResolvedTurnPermissions;
pub use state::ThreadPermissionState;
pub use store::ApprovalCache;
pub use store::ApprovalRecord;
pub use store::PendingApproval;
pub use store::PendingApprovalStore;
pub use sync::PermissionSync;
