use super::super::convert::ExternalSessionBuilder;
use super::super::record::ExternalSessionRecord;
use super::model::CursorBubble;
use super::model::CursorBubbleHeader;
use super::model::CursorBubbleRole;
use super::model::CursorThreadHead;
use std::collections::HashMap;

pub(super) fn build_record(
    head: &CursorThreadHead,
    headers: &[CursorBubbleHeader],
    bubble_values: &HashMap<String, String>,
) -> Option<ExternalSessionRecord> {
    let created_at = head.created_or_updated_at();
    let mut session = ExternalSessionBuilder::new(
        super::SOURCE,
        head.external_id(),
        head.title(),
        Some(head.cwd()),
        created_at,
    )?;

    for header in headers {
        let raw_bubble = match head.raw_bubble(bubble_values, header) {
            Some(raw_bubble) => raw_bubble,
            None => continue,
        };
        let Some(bubble) = CursorBubble::parse(raw_bubble, header.kind()) else {
            continue;
        };
        let timestamp = bubble.timestamp_or_else(|| head.fallback_timestamp(created_at));
        match bubble.role() {
            CursorBubbleRole::User => session.push_user_message(timestamp, bubble.text()),
            CursorBubbleRole::Agent => session.push_agent_message(timestamp, bubble.text()),
        }
    }

    session.finish()
}
