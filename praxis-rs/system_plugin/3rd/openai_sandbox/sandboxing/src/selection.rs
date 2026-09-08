#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SandboxType {
    None,
    MacosSeatbelt,
    LinuxSeccomp,
    WindowsRestrictedToken,
}

impl SandboxType {
    pub fn as_metric_tag(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::MacosSeatbelt => "seatbelt",
            Self::LinuxSeccomp => "seccomp",
            Self::WindowsRestrictedToken => "windows_sandbox",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SandboxablePreference {
    Auto,
    Require,
    Forbid,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SandboxSelectionError {
    RequiredSandboxUnavailable,
}

impl std::fmt::Display for SandboxSelectionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::RequiredSandboxUnavailable => f.write_str(
                "execution requires a platform sandbox, but none is enabled or supported; refusing to run unsandboxed",
            ),
        }
    }
}

impl std::error::Error for SandboxSelectionError {}

pub(crate) fn select_sandbox(
    preference: SandboxablePreference,
    policy_requires_sandbox: bool,
    platform: Option<SandboxType>,
) -> Result<SandboxType, SandboxSelectionError> {
    match preference {
        SandboxablePreference::Forbid => Ok(SandboxType::None),
        SandboxablePreference::Auto if !policy_requires_sandbox => Ok(SandboxType::None),
        SandboxablePreference::Auto | SandboxablePreference::Require => platform
            .filter(|backend| *backend != SandboxType::None)
            .ok_or(SandboxSelectionError::RequiredSandboxUnavailable),
    }
}

#[cfg(test)]
#[path = "selection_tests.rs"]
mod tests;
