#[cfg(target_os = "linux")]
mod bwrap;
pub mod landlock;
mod manager;
pub mod policy_transforms;
#[cfg(target_os = "macos")]
pub mod seatbelt;
mod selection;
mod windows_policy;

#[cfg(target_os = "linux")]
pub use bwrap::find_system_bwrap_in_path;
#[cfg(target_os = "linux")]
pub use bwrap::system_bwrap_warning;
pub use manager::SandboxCommand;
pub use manager::SandboxExecRequest;
pub use manager::SandboxManager;
pub use manager::SandboxTransformError;
pub use manager::SandboxTransformRequest;
pub use manager::get_platform_sandbox;
pub use selection::SandboxSelectionError;
pub use selection::SandboxType;
pub use selection::SandboxablePreference;

#[cfg(not(target_os = "linux"))]
pub fn system_bwrap_warning() -> Option<String> {
    None
}

pub use windows_policy::WindowsRestrictedTokenFilesystemOverlay;
pub use windows_policy::resolve_windows_restricted_token_filesystem_overlay;
