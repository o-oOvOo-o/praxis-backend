/// One incremental projection of model text.
///
/// `renderable` may be forwarded to a UI immediately. `events` carries
/// structured data removed from that text, so hosts can route it without
/// teaching the renderer about model-side markup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextProjection<E> {
    pub renderable: String,
    pub events: Vec<E>,
}

impl<E> Default for TextProjection<E> {
    fn default() -> Self {
        Self {
            renderable: String::new(),
            events: Vec::new(),
        }
    }
}

impl<E> TextProjection<E> {
    pub fn renderable(text: impl Into<String>) -> Self {
        Self {
            renderable: text.into(),
            events: Vec::new(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.renderable.is_empty() && self.events.is_empty()
    }

    pub fn map_events<U>(self, map: impl FnMut(E) -> U) -> TextProjection<U> {
        TextProjection {
            renderable: self.renderable,
            events: self.events.into_iter().map(map).collect(),
        }
    }

    pub fn merge(&mut self, mut next: Self) {
        self.renderable.push_str(&next.renderable);
        self.events.append(&mut next.events);
    }
}

/// A host-composable stage that projects streamed model text into renderable
/// text and typed events.
pub trait TextProjector {
    type Event;

    fn project(&mut self, input: &str) -> TextProjection<Self::Event>;

    /// Close this stream and drain state. Calling `close` again must be safe.
    fn close(&mut self) -> TextProjection<Self::Event>;
}
