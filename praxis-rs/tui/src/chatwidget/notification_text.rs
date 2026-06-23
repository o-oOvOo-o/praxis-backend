use std::path::PathBuf;

use praxis_config::types::Notifications;
use praxis_protocol::request_user_input::RequestUserInputQuestion;

use crate::diff_render::display_path_for;
use crate::text_formatting::truncate_text;

const AGENT_NOTIFICATION_PREVIEW_GRAPHEMES: usize = 200;

#[derive(Debug, Clone)]
pub(crate) enum Notification {
    AgentTurnComplete {
        response: String,
    },
    ExecApprovalRequested {
        command: String,
    },
    EditApprovalRequested {
        cwd: PathBuf,
        changes: Vec<PathBuf>,
    },
    ElicitationRequested {
        server_name: String,
    },
    PlanModePrompt {
        title: String,
    },
    UserInputRequested {
        question_count: usize,
        summary: Option<String>,
    },
}

impl Notification {
    pub(crate) fn display(&self) -> String {
        match self {
            Notification::AgentTurnComplete { response } => {
                Notification::agent_turn_preview(response)
                    .unwrap_or_else(|| "Agent turn complete".to_string())
            }
            Notification::ExecApprovalRequested { command } => {
                format!(
                    "Approval requested: {}",
                    truncate_text(command, /*max_graphemes*/ 30)
                )
            }
            Notification::EditApprovalRequested { cwd, changes } => {
                format!(
                    "Praxis wants to edit {}",
                    if changes.len() == 1 {
                        #[allow(clippy::unwrap_used)]
                        display_path_for(changes.first().unwrap(), cwd)
                    } else {
                        format!("{} files", changes.len())
                    }
                )
            }
            Notification::ElicitationRequested { server_name } => {
                format!("Approval requested by {server_name}")
            }
            Notification::PlanModePrompt { title } => {
                format!("Plan mode prompt: {title}")
            }
            Notification::UserInputRequested {
                question_count,
                summary,
            } => match (*question_count, summary.as_deref()) {
                (1, Some(summary)) => format!("Question requested: {summary}"),
                (1, None) => "Question requested".to_string(),
                (count, _) => format!("Questions requested: {count}"),
            },
        }
    }

    pub(crate) fn type_name(&self) -> &str {
        match self {
            Notification::AgentTurnComplete { .. } => "agent-turn-complete",
            Notification::ExecApprovalRequested { .. }
            | Notification::EditApprovalRequested { .. }
            | Notification::ElicitationRequested { .. } => "approval-requested",
            Notification::PlanModePrompt { .. } => "plan-mode-prompt",
            Notification::UserInputRequested { .. } => "user-input-requested",
        }
    }

    pub(crate) fn priority(&self) -> u8 {
        match self {
            Notification::AgentTurnComplete { .. } => 0,
            Notification::ExecApprovalRequested { .. }
            | Notification::EditApprovalRequested { .. }
            | Notification::ElicitationRequested { .. }
            | Notification::PlanModePrompt { .. }
            | Notification::UserInputRequested { .. } => 1,
        }
    }

    pub(crate) fn allowed_for(&self, settings: &Notifications) -> bool {
        match settings {
            Notifications::Enabled(enabled) => *enabled,
            Notifications::Custom(allowed) => allowed.iter().any(|a| a == self.type_name()),
        }
    }

    pub(crate) fn user_input_request_summary(
        questions: &[RequestUserInputQuestion],
    ) -> Option<String> {
        let first_question = questions.first()?;
        let summary = if first_question.header.trim().is_empty() {
            first_question.question.trim()
        } else {
            first_question.header.trim()
        };
        if summary.is_empty() {
            None
        } else {
            Some(truncate_text(summary, /*max_graphemes*/ 30))
        }
    }

    fn agent_turn_preview(response: &str) -> Option<String> {
        let mut normalized = String::new();
        for part in response.split_whitespace() {
            if !normalized.is_empty() {
                normalized.push(' ');
            }
            normalized.push_str(part);
        }
        let trimmed = normalized.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(truncate_text(trimmed, AGENT_NOTIFICATION_PREVIEW_GRAPHEMES))
        }
    }
}
