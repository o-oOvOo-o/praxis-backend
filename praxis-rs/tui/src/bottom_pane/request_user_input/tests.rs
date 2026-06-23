use super::*;
use crate::app_event::AppEvent;
use crate::bottom_pane::selection_popup_common::menu_surface_inset;
use crate::render::renderable::Renderable;
use praxis_protocol::request_user_input::RequestUserInputQuestion;
use praxis_protocol::request_user_input::RequestUserInputQuestionOption;
use pretty_assertions::assert_eq;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use std::collections::HashMap;
use tokio::sync::mpsc::unbounded_channel;
use unicode_width::UnicodeWidthStr;

fn test_sender() -> (
    AppEventSender,
    tokio::sync::mpsc::UnboundedReceiver<AppEvent>,
) {
    let (tx_raw, rx) = unbounded_channel::<AppEvent>();
    (AppEventSender::new(tx_raw), rx)
}

fn expect_interrupt_only(rx: &mut tokio::sync::mpsc::UnboundedReceiver<AppEvent>) {
    let event = rx.try_recv().expect("expected interrupt AppEvent");
    let AppEvent::AgentOp(op) = event else {
        panic!("expected AgentOp");
    };
    assert_eq!(op, Op::Interrupt);
    assert!(
        rx.try_recv().is_err(),
        "unexpected AppEvents before interrupt completion"
    );
}

fn question_with_options(id: &str, header: &str) -> RequestUserInputQuestion {
    RequestUserInputQuestion {
        id: id.to_string(),
        header: header.to_string(),
        question: "Choose an option.".to_string(),
        is_other: false,
        is_secret: false,
        options: Some(vec![
            RequestUserInputQuestionOption {
                label: "Option 1".to_string(),
                description: "First choice.".to_string(),
            },
            RequestUserInputQuestionOption {
                label: "Option 2".to_string(),
                description: "Second choice.".to_string(),
            },
            RequestUserInputQuestionOption {
                label: "Option 3".to_string(),
                description: "Third choice.".to_string(),
            },
        ]),
    }
}

fn question_with_options_and_other(id: &str, header: &str) -> RequestUserInputQuestion {
    RequestUserInputQuestion {
        id: id.to_string(),
        header: header.to_string(),
        question: "Choose an option.".to_string(),
        is_other: true,
        is_secret: false,
        options: Some(vec![
            RequestUserInputQuestionOption {
                label: "Option 1".to_string(),
                description: "First choice.".to_string(),
            },
            RequestUserInputQuestionOption {
                label: "Option 2".to_string(),
                description: "Second choice.".to_string(),
            },
            RequestUserInputQuestionOption {
                label: "Option 3".to_string(),
                description: "Third choice.".to_string(),
            },
        ]),
    }
}

fn question_with_wrapped_options(id: &str, header: &str) -> RequestUserInputQuestion {
    RequestUserInputQuestion {
        id: id.to_string(),
        header: header.to_string(),
        question: "Choose the next step for this task.".to_string(),
        is_other: false,
        is_secret: false,
        options: Some(vec![
            RequestUserInputQuestionOption {
                label: "Discuss a code change".to_string(),
                description: "Walk through a plan, then implement it together with careful checks."
                    .to_string(),
            },
            RequestUserInputQuestionOption {
                label: "Run targeted tests".to_string(),
                description:
                    "Pick the most relevant crate and validate the current behavior first."
                        .to_string(),
            },
            RequestUserInputQuestionOption {
                label: "Review the diff".to_string(),
                description:
                    "Summarize the changes and highlight the most important risks and gaps."
                        .to_string(),
            },
        ]),
    }
}

fn question_with_very_long_option_text(id: &str, header: &str) -> RequestUserInputQuestion {
    RequestUserInputQuestion {
            id: id.to_string(),
            header: header.to_string(),
            question: "Choose one option.".to_string(),
            is_other: false,
            is_secret: false,
            options: Some(vec![
                RequestUserInputQuestionOption {
                    label: "Job: running/completed/failed/expired; Run/Experiment: succeeded/failed/unknown (Recommended when triaging long-running background work and status transitions)".to_string(),
                    description: "Keep async job statuses for progress tracking and include enough context for debugging retries, stale workers, and unexpected expiration paths.".to_string(),
                },
                RequestUserInputQuestionOption {
                    label: "Add a short status model".to_string(),
                    description: "Simpler labels with less detail for quick rollouts.".to_string(),
                },
            ]),
        }
}

fn question_with_long_scroll_options(id: &str, header: &str) -> RequestUserInputQuestion {
    RequestUserInputQuestion {
            id: id.to_string(),
            header: header.to_string(),
            question:
                "Choose one option; each hint is intentionally very long to test wrapped scrolling."
                    .to_string(),
            is_other: false,
            is_secret: false,
            options: Some(vec![
                RequestUserInputQuestionOption {
                    label: "Use Detailed Hint A (Recommended)".to_string(),
                    description: "Select this if you want a deliberately overextended explanatory hint that reads like a miniature specification, including context, rationale, expected behavior, and an explicit statement that this choice is mainly for testing how gracefully the interface wraps, truncates, and preserves readability under unusually verbose helper text conditions.".to_string(),
                },
                RequestUserInputQuestionOption {
                    label: "Use Detailed Hint B".to_string(),
                    description: "Select this if you want an equally verbose but differently phrased guidance block that emphasizes user-facing clarity, spacing tolerance, multiline wrapping, visual hierarchy interactions, and whether long descriptive metadata remains understandable when scanned quickly in a constrained layout where cognitive load is already high.".to_string(),
                },
                RequestUserInputQuestionOption {
                    label: "Use Detailed Hint C".to_string(),
                    description: "Select this when you specifically want to verify that navigating downward will keep the currently highlighted option visible, even when previous options consume many wrapped lines and would otherwise push the selection out of the viewport.".to_string(),
                },
                RequestUserInputQuestionOption {
                    label: "None of the above".to_string(),
                    description:
                        "Use this only if the previous long-form options do not apply.".to_string(),
                },
            ]),
        }
}

fn question_without_options(id: &str, header: &str) -> RequestUserInputQuestion {
    RequestUserInputQuestion {
        id: id.to_string(),
        header: header.to_string(),
        question: "Share details.".to_string(),
        is_other: false,
        is_secret: false,
        options: None,
    }
}

fn request_event(turn_id: &str, questions: Vec<RequestUserInputQuestion>) -> RequestUserInputEvent {
    RequestUserInputEvent {
        call_id: "call-1".to_string(),
        turn_id: turn_id.to_string(),
        questions,
    }
}

fn snapshot_buffer(buf: &Buffer) -> String {
    let mut lines = Vec::new();
    for y in 0..buf.area().height {
        let mut row = String::new();
        for x in 0..buf.area().width {
            row.push(buf[(x, y)].symbol().chars().next().unwrap_or(' '));
        }
        lines.push(row);
    }
    lines.join("\n")
}

fn render_snapshot(overlay: &RequestUserInputOverlay, area: Rect) -> String {
    let mut buf = Buffer::empty(area);
    overlay.render(area, &mut buf);
    snapshot_buffer(&buf)
}

mod cancel_unanswered;
mod navigation_focus;
mod notes_paste;
mod queue_submission;
mod render_layout;
