use std::sync::Arc;

use praxis_protocol::models::ContentItem;
use praxis_protocol::models::ResponseItem;
use praxis_protocol::protocol::EventMsg;
use praxis_protocol::protocol::ExitedReviewModeEvent;
use praxis_protocol::protocol::ReviewOutputEvent;

use crate::praxis::Session;
use crate::praxis::TurnContext;
use crate::review_format::format_review_findings_block;
use crate::review_format::render_review_output_text;

use super::templates;

pub(super) async fn exit_review_mode(
    session: Arc<Session>,
    review_output: Option<ReviewOutputEvent>,
    ctx: Arc<TurnContext>,
) {
    const REVIEW_USER_MESSAGE_ID: &str = "review_rollout_user";
    const REVIEW_ASSISTANT_MESSAGE_ID: &str = "review_rollout_assistant";
    let (user_message, assistant_message) = if let Some(out) = review_output.clone() {
        let mut findings_str = String::new();
        let text = out.overall_explanation.trim();
        if !text.is_empty() {
            findings_str.push_str(text);
        }
        if !out.findings.is_empty() {
            let block = format_review_findings_block(&out.findings, /*selection*/ None);
            findings_str.push_str(&format!("\n{block}"));
        }
        let rendered = templates::render_review_exit_success(&findings_str);
        let assistant_message = render_review_output_text(&out);
        (rendered, assistant_message)
    } else {
        let rendered = templates::normalize_review_template_line_endings(
            crate::client_common::REVIEW_EXIT_INTERRUPTED_TMPL,
        )
        .into_owned();
        let assistant_message =
            "Review was interrupted. Please re-run /review and wait for it to complete."
                .to_string();
        (rendered, assistant_message)
    };

    session
        .record_conversation_items(
            &ctx,
            &[ResponseItem::Message {
                id: Some(REVIEW_USER_MESSAGE_ID.to_string()),
                role: "user".to_string(),
                content: vec![ContentItem::InputText { text: user_message }],
                end_turn: None,
                phase: None,
            }],
        )
        .await;

    session
        .send_event(
            ctx.as_ref(),
            EventMsg::ExitedReviewMode(ExitedReviewModeEvent { review_output }),
        )
        .await;
    session
        .record_response_item_and_emit_turn_item(
            ctx.as_ref(),
            ResponseItem::Message {
                id: Some(REVIEW_ASSISTANT_MESSAGE_ID.to_string()),
                role: "assistant".to_string(),
                content: vec![ContentItem::OutputText {
                    text: assistant_message,
                }],
                end_turn: None,
                phase: None,
            },
        )
        .await;

    session.ensure_rollout_materialized().await;
}
