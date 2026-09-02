use serde::Deserialize;
use serde::Serialize;
use std::sync::Arc;
use std::sync::Mutex;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum EffectAccess {
    Read,
    Write,
    Exclusive,
}

impl EffectAccess {
    fn conflicts(self, other: Self) -> bool {
        !matches!((self, other), (Self::Read, Self::Read))
    }

    fn covers(self, actual: Self) -> bool {
        match self {
            Self::Read => actual == Self::Read,
            Self::Write | Self::Exclusive => true,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct EffectKey {
    domain: String,
    segments: Vec<String>,
}

impl EffectKey {
    pub fn root(domain: impl Into<String>) -> Self {
        Self {
            domain: domain.into(),
            segments: Vec::new(),
        }
    }

    pub fn hierarchical(
        domain: impl Into<String>,
        segments: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        Self {
            domain: domain.into(),
            segments: segments.into_iter().map(Into::into).collect(),
        }
    }

    pub fn domain(&self) -> &str {
        &self.domain
    }

    pub fn segments(&self) -> &[String] {
        &self.segments
    }

    fn overlaps(&self, other: &Self) -> bool {
        (self.domain == "*" || other.domain == "*" || self.domain == other.domain)
            && (is_prefix(&self.segments, &other.segments)
                || is_prefix(&other.segments, &self.segments))
    }

    fn covers(&self, actual: &Self) -> bool {
        (self.domain == "*" || self.domain == actual.domain)
            && is_prefix(&self.segments, &actual.segments)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ToolEffect {
    pub key: EffectKey,
    pub access: EffectAccess,
}

impl ToolEffect {
    pub fn read(key: EffectKey) -> Self {
        Self {
            key,
            access: EffectAccess::Read,
        }
    }

    pub fn write(key: EffectKey) -> Self {
        Self {
            key,
            access: EffectAccess::Write,
        }
    }

    pub fn exclusive(key: EffectKey) -> Self {
        Self {
            key,
            access: EffectAccess::Exclusive,
        }
    }

    fn conflicts(&self, other: &Self) -> bool {
        self.key.overlaps(&other.key) && self.access.conflicts(other.access)
    }

    fn covers(&self, actual: &Self) -> bool {
        self.key.covers(&actual.key) && self.access.covers(actual.access)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ToolEffects {
    effects: Vec<ToolEffect>,
}

impl Default for ToolEffects {
    fn default() -> Self {
        Self::unknown_write()
    }
}

impl ToolEffects {
    pub fn new(effects: impl IntoIterator<Item = ToolEffect>) -> Self {
        Self {
            effects: effects.into_iter().collect(),
        }
    }

    pub fn read(key: EffectKey) -> Self {
        Self::new([ToolEffect::read(key)])
    }

    pub fn pure() -> Self {
        Self {
            effects: Vec::new(),
        }
    }

    pub fn write(key: EffectKey) -> Self {
        Self::new([ToolEffect::write(key)])
    }

    pub fn exclusive(key: EffectKey) -> Self {
        Self::new([ToolEffect::exclusive(key)])
    }

    pub fn unknown_read() -> Self {
        Self::read(EffectKey::root("*"))
    }

    pub fn unknown_write() -> Self {
        Self::write(EffectKey::root("*"))
    }

    pub fn iter(&self) -> impl Iterator<Item = &ToolEffect> {
        self.effects.iter()
    }

    pub fn conflicts(&self, other: &Self) -> bool {
        self.effects
            .iter()
            .any(|left| other.effects.iter().any(|right| left.conflicts(right)))
    }

    pub fn covers(&self, actual: &ToolEffect) -> bool {
        self.effects.iter().any(|planned| planned.covers(actual))
    }
}

#[derive(Clone, Debug, Default)]
pub struct EffectJournal {
    effects: Arc<Mutex<Vec<ToolEffect>>>,
}

impl EffectJournal {
    pub fn record(&self, effect: ToolEffect) {
        let mut effects = self.effects.lock().expect("effect journal lock poisoned");
        if !effects.contains(&effect) {
            effects.push(effect);
        }
    }

    pub fn record_all(&self, effects: &ToolEffects) {
        let mut recorded = self.effects.lock().expect("effect journal lock poisoned");
        for effect in effects.iter() {
            if !recorded.contains(effect) {
                recorded.push(effect.clone());
            }
        }
    }

    pub fn snapshot(&self) -> ToolEffects {
        ToolEffects::new(
            self.effects
                .lock()
                .expect("effect journal lock poisoned")
                .clone(),
        )
    }

    pub fn validate(&self, planned: &ToolEffects) -> EffectValidation {
        let observed = self.snapshot();
        let unexpected = observed
            .iter()
            .filter(|effect| !planned.covers(effect))
            .cloned()
            .collect();
        EffectValidation {
            observed,
            unexpected,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EffectValidation {
    pub observed: ToolEffects,
    pub unexpected: Vec<ToolEffect>,
}

impl EffectValidation {
    pub fn is_valid(&self) -> bool {
        self.unexpected.is_empty()
    }
}

fn is_prefix(prefix: &[String], value: &[String]) -> bool {
    prefix.len() <= value.len() && prefix.iter().zip(value).all(|(left, right)| left == right)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(parts: &[&str]) -> EffectKey {
        EffectKey::hierarchical("filesystem", parts.iter().copied())
    }

    #[test]
    fn sibling_writes_do_not_conflict() {
        let left = ToolEffects::write(file(&["repo", "a.rs"]));
        let right = ToolEffects::write(file(&["repo", "b.rs"]));
        assert!(!left.conflicts(&right));
    }

    #[test]
    fn parent_and_child_conflict_when_one_writes() {
        let parent = ToolEffects::read(file(&["repo", "src"]));
        let child = ToolEffects::write(file(&["repo", "src", "main.rs"]));
        assert!(parent.conflicts(&child));
    }

    #[test]
    fn reads_of_the_same_resource_can_overlap() {
        let left = ToolEffects::read(file(&["repo", "main.rs"]));
        let right = ToolEffects::read(file(&["repo", "main.rs"]));
        assert!(!left.conflicts(&right));
    }

    #[test]
    fn unknown_write_conflicts_with_every_domain() {
        let unknown = ToolEffects::unknown_write();
        let file_read = ToolEffects::read(file(&["repo", "main.rs"]));
        let graph_write = ToolEffects::write(EffectKey::hierarchical(
            "cunning3d.graph",
            ["main", "node-42"],
        ));
        assert!(unknown.conflicts(&file_read));
        assert!(unknown.conflicts(&graph_write));
    }

    #[test]
    fn parent_write_covers_child_read_and_write() {
        let planned = ToolEffects::write(file(&["repo", "src"]));
        assert!(planned.covers(&ToolEffect::read(file(&["repo", "src", "main.rs"]))));
        assert!(planned.covers(&ToolEffect::write(file(&["repo", "src", "main.rs"]))));
    }

    #[test]
    fn read_does_not_cover_write() {
        let planned = ToolEffects::read(file(&["repo", "main.rs"]));
        assert!(!planned.covers(&ToolEffect::write(file(&["repo", "main.rs"]))));
    }

    #[test]
    fn journal_reports_effects_outside_the_plan() {
        let journal = EffectJournal::default();
        journal.record(ToolEffect::write(file(&["repo", "other.rs"])));
        let validation = journal.validate(&ToolEffects::write(file(&["repo", "main.rs"])));
        assert!(!validation.is_valid());
        assert_eq!(validation.unexpected.len(), 1);
    }

    #[test]
    fn journal_records_a_batch_once() {
        let effect = ToolEffect::write(file(&["repo", "main.rs"]));
        let journal = EffectJournal::default();
        journal.record(effect.clone());
        journal.record_all(&ToolEffects::new([effect]));

        assert_eq!(journal.snapshot().iter().count(), 1);
    }
}
