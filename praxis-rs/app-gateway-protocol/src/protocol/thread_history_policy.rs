use praxis_protocol::models::MessagePhase;

use super::api::ThreadItem;
use super::api::UserInput;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResumedHistoryLane {
    User,
    Assistant,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResumedHistoryLabel {
    You,
    Assistant,
    AssistantNote,
    Plan,
    Reasoning,
    Review,
    ReviewExited,
    Context,
    Hook,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ResumedThreadHistoryAction {
    Show {
        lane: ResumedHistoryLane,
        label: ResumedHistoryLabel,
        preview: String,
    },
    FoldToolEvent,
    Drop,
}

pub fn classify_resumed_thread_item(item: ThreadItem) -> ResumedThreadHistoryAction {
    match item {
        ThreadItem::UserMessage { content, .. } => ResumedThreadHistoryAction::Show {
            lane: ResumedHistoryLane::User,
            label: ResumedHistoryLabel::You,
            preview: user_input_preview(content),
        },
        ThreadItem::AgentMessage { text, phase, .. } => {
            let label = match phase {
                Some(MessagePhase::Commentary) => ResumedHistoryLabel::AssistantNote,
                Some(MessagePhase::FinalAnswer) | None => ResumedHistoryLabel::Assistant,
            };
            ResumedThreadHistoryAction::Show {
                lane: ResumedHistoryLane::Assistant,
                label,
                preview: text,
            }
        }
        ThreadItem::Plan { text, .. } => ResumedThreadHistoryAction::Show {
            lane: ResumedHistoryLane::Assistant,
            label: ResumedHistoryLabel::Plan,
            preview: text,
        },
        ThreadItem::Reasoning {
            summary, content, ..
        } => {
            let preview = summary
                .into_iter()
                .next()
                .or_else(|| content.into_iter().next())
                .unwrap_or_default();
            if preview.trim().is_empty() {
                ResumedThreadHistoryAction::Drop
            } else {
                ResumedThreadHistoryAction::Show {
                    lane: ResumedHistoryLane::Assistant,
                    label: ResumedHistoryLabel::Reasoning,
                    preview,
                }
            }
        }
        ThreadItem::EnteredReviewMode { review, .. } => ResumedThreadHistoryAction::Show {
            lane: ResumedHistoryLane::Assistant,
            label: ResumedHistoryLabel::Review,
            preview: review,
        },
        ThreadItem::ExitedReviewMode { review, .. } => ResumedThreadHistoryAction::Show {
            lane: ResumedHistoryLane::Assistant,
            label: ResumedHistoryLabel::ReviewExited,
            preview: review,
        },
        ThreadItem::ContextCompaction { .. } => ResumedThreadHistoryAction::Show {
            lane: ResumedHistoryLane::Assistant,
            label: ResumedHistoryLabel::Context,
            preview: "Compacted conversation history".to_string(),
        },
        ThreadItem::HookPrompt { fragments, .. } => {
            let preview = fragments
                .into_iter()
                .map(|fragment| fragment.text)
                .collect::<Vec<_>>()
                .join(" ");
            if preview.trim().is_empty() {
                ResumedThreadHistoryAction::Drop
            } else {
                ResumedThreadHistoryAction::Show {
                    lane: ResumedHistoryLane::Assistant,
                    label: ResumedHistoryLabel::Hook,
                    preview,
                }
            }
        }
        ThreadItem::CommandExecution { .. }
        | ThreadItem::FileChange { .. }
        | ThreadItem::McpToolCall { .. }
        | ThreadItem::DynamicToolCall { .. }
        | ThreadItem::CollabAgentToolCall { .. }
        | ThreadItem::WebSearch { .. }
        | ThreadItem::ImageView { .. }
        | ThreadItem::ImageGeneration { .. } => ResumedThreadHistoryAction::FoldToolEvent,
    }
}

fn user_input_preview(content: Vec<UserInput>) -> String {
    let mut parts = Vec::new();
    for item in content {
        match item {
            UserInput::Text { text, .. } => {
                if !text.trim().is_empty() {
                    parts.push(text);
                }
            }
            UserInput::Image { url } => {
                parts.push(format!("image {url}"));
            }
            UserInput::LocalImage { path } => {
                parts.push(format!("image {}", path.display()));
            }
            UserInput::Skill { name, .. } => {
                parts.push(format!("skill {name}"));
            }
            UserInput::Mention { name, .. } => {
                parts.push(format!("@{name}"));
            }
        }
    }
    parts.join(" ")
}
