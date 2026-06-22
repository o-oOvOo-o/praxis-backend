use praxis_protocol::models::ContentItem;
use praxis_protocol::models::ResponseItem;

pub(super) struct ResponseMessageBuffer {
    pending_role: Option<String>,
    pending_content: Vec<ContentItem>,
    items: Vec<ResponseItem>,
}

impl ResponseMessageBuffer {
    pub(super) fn new() -> Self {
        Self {
            pending_role: None,
            pending_content: Vec::new(),
            items: Vec::new(),
        }
    }

    pub(super) fn push_content(&mut self, role: &str, content: ContentItem) {
        if self
            .pending_role
            .as_deref()
            .is_some_and(|pending| pending != role)
        {
            self.flush();
        }
        if self.pending_role.is_none() {
            self.pending_role = Some(role.to_string());
        }
        self.pending_content.push(content);
    }

    pub(super) fn push_item(&mut self, item: ResponseItem) {
        self.flush();
        self.items.push(item);
    }

    pub(super) fn finish(mut self) -> Vec<ResponseItem> {
        self.flush();
        self.items
    }

    fn flush(&mut self) {
        let Some(role) = self.pending_role.take() else {
            return;
        };
        if self.pending_content.is_empty() {
            return;
        }
        self.items.push(ResponseItem::Message {
            id: None,
            role,
            content: std::mem::take(&mut self.pending_content),
            end_turn: None,
            phase: None,
        });
    }
}
