use serde::Deserialize;
use serde::Deserializer;
use serde::Serialize;
use serde::Serializer;

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct TurnDiffTracker {
    #[serde(rename = "has_changes")]
    status: TurnDiffStatus,
}

impl TurnDiffTracker {
    pub fn mark_changed(&mut self) {
        self.status = TurnDiffStatus::changed();
    }

    pub fn has_changes(&self) -> bool {
        self.status.has_changes()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TurnDiffStatus {
    Clean,
    Changed,
}

impl Default for TurnDiffStatus {
    fn default() -> Self {
        Self::Clean
    }
}

impl Serialize for TurnDiffStatus {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_bool(self.has_changes())
    }
}

impl<'de> Deserialize<'de> for TurnDiffStatus {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        bool::deserialize(deserializer).map(Self::from_has_changes)
    }
}

impl TurnDiffStatus {
    fn changed() -> Self {
        Self::Changed
    }

    fn from_has_changes(has_changes: bool) -> Self {
        if has_changes {
            Self::Changed
        } else {
            Self::Clean
        }
    }

    fn has_changes(self) -> bool {
        matches!(self, Self::Changed)
    }
}
