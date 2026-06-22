use serde::Deserialize;
use serde::Deserializer;
use serde::Serialize;
use serde::Serializer;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ToolResultStatus {
    Success,
    Error,
}

impl Default for ToolResultStatus {
    fn default() -> Self {
        Self::success()
    }
}

impl Serialize for ToolResultStatus {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_bool(self.is_error())
    }
}

impl<'de> Deserialize<'de> for ToolResultStatus {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        bool::deserialize(deserializer).map(Self::from_error_flag)
    }
}

impl ToolResultStatus {
    pub fn success() -> Self {
        Self::Success
    }

    pub fn error() -> Self {
        Self::Error
    }

    fn from_error_flag(is_error: bool) -> Self {
        if is_error {
            Self::error()
        } else {
            Self::success()
        }
    }

    pub fn from_success_flag(success: bool) -> Self {
        if success {
            Self::success()
        } else {
            Self::error()
        }
    }

    pub fn is_error(self) -> bool {
        matches!(self, Self::Error)
    }

    pub fn is_success(self) -> bool {
        matches!(self, Self::Success)
    }
}
