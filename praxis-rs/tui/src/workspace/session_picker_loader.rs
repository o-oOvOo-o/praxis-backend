use std::collections::HashMap;
use std::fmt;

use crate::SessionLookupSource;
use tokio::sync::mpsc;

use super::session_picker::SessionPickerPageRequest;

#[derive(Default)]
pub(crate) struct SessionPickerPageLoaders {
    senders: HashMap<SessionLookupSource, mpsc::UnboundedSender<SessionPickerPageRequest>>,
}

impl SessionPickerPageLoaders {
    pub(crate) fn clear(&mut self) {
        self.senders.clear();
    }

    pub(crate) fn contains_source(&self, source: SessionLookupSource) -> bool {
        self.senders.contains_key(&source)
    }

    pub(crate) fn insert(
        &mut self,
        source: SessionLookupSource,
        sender: mpsc::UnboundedSender<SessionPickerPageRequest>,
    ) {
        self.senders.insert(source, sender);
    }

    pub(crate) fn queue(
        &mut self,
        request: SessionPickerPageRequest,
    ) -> Option<(SessionPickerPageRequest, String)> {
        let source = request.source;
        let Some(sender) = self.senders.get(&source) else {
            return Some((
                request,
                format!(
                    "{} picker worker was not ready before loading threads.",
                    source.display_name()
                ),
            ));
        };

        if sender.send(request.clone()).is_ok() {
            return None;
        }

        self.senders.remove(&source);
        Some((
            request,
            format!(
                "{} picker worker stopped before loading threads.",
                source.display_name()
            ),
        ))
    }
}

impl fmt::Debug for SessionPickerPageLoaders {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SessionPickerPageLoaders")
            .field("sources", &self.senders.keys().collect::<Vec<_>>())
            .finish()
    }
}
