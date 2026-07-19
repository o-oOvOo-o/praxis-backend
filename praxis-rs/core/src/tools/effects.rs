use std::future::Future;
use std::path::Component;
use std::path::Path;

use praxis_loop::tool::EffectJournal;
use praxis_loop::tool::EffectKey;
use praxis_loop::tool::ToolEffect;

tokio::task_local! {
    static ACTIVE_EFFECT_JOURNAL: EffectJournal;
}

pub(crate) async fn scope_effect_journal<F>(journal: EffectJournal, future: F) -> F::Output
where
    F: Future,
{
    ACTIVE_EFFECT_JOURNAL.scope(journal, future).await
}

pub(crate) fn record_effect(effect: ToolEffect) {
    let _ = ACTIVE_EFFECT_JOURNAL.try_with(|journal| journal.record(effect));
}

pub(crate) fn record_filesystem_read(path: &Path) {
    record_effect(ToolEffect::read(filesystem_effect_key(path)));
}

pub(crate) fn record_filesystem_write(path: &Path) {
    record_effect(ToolEffect::write(filesystem_effect_key(path)));
}

pub(crate) fn filesystem_effect_key(path: &Path) -> EffectKey {
    let mut segments = Vec::new();
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => {
                segments.push(prefix.as_os_str().to_string_lossy().to_lowercase());
            }
            Component::RootDir => {}
            Component::CurDir => {}
            Component::ParentDir => {
                segments.pop();
            }
            Component::Normal(segment) => {
                let segment = segment.to_string_lossy();
                #[cfg(windows)]
                segments.push(segment.to_lowercase());
                #[cfg(not(windows))]
                segments.push(segment.into_owned());
            }
        }
    }
    EffectKey::hierarchical("filesystem", segments)
}

pub(crate) fn conversation_effect_key<I, S>(
    conversation_id: impl std::fmt::Display,
    resource: I,
) -> EffectKey
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let mut segments = vec!["conversation".to_string(), conversation_id.to_string()];
    segments.extend(resource.into_iter().map(Into::into));
    EffectKey::hierarchical("praxis", segments)
}

pub(crate) fn service_effect_key(service: &str) -> EffectKey {
    EffectKey::hierarchical("service", [service])
}
