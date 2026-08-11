use ratatui::style::Stylize;
use ratatui::text::Line;

use super::ChatWidget;
use crate::app_event::AppEvent;
use crate::bottom_pane::SelectionAction;
use crate::bottom_pane::SelectionItem;
use crate::bottom_pane::SelectionViewParams;
use crate::bottom_pane::custom_prompt_view::CustomPromptView;
use crate::bottom_pane::popup_consts::standard_popup_hint_line;
use crate::history_cell;
use crate::provider_setup::ProviderSetupKind;
use crate::render::renderable::ColumnRenderable;

impl ChatWidget {
    pub(crate) fn open_login_popup(&mut self) {
        let openai_actions: Vec<SelectionAction> = vec![Box::new(|tx| {
            tx.send(AppEvent::InsertHistoryCell(Box::new(
                history_cell::new_info_event(
                    "Praxis uses your ChatGPT/OpenAI login when it is available.".to_string(),
                    Some(
                        "If no provider works at startup, Praxis opens the full ChatGPT sign-in flow."
                            .to_string(),
                    ),
                ),
            )));
        })];
        let openai_api_actions: Vec<SelectionAction> = vec![Box::new(|tx| {
            tx.send(AppEvent::OpenProviderLoginPrompt {
                provider: ProviderSetupKind::OpenAi,
            });
        })];
        let responses_api_actions: Vec<SelectionAction> = vec![Box::new(|tx| {
            tx.send(AppEvent::OpenProviderLoginPrompt {
                provider: ProviderSetupKind::ResponsesApi,
            });
        })];
        let claude_api_actions: Vec<SelectionAction> = vec![Box::new(|tx| {
            tx.send(AppEvent::OpenProviderLoginPrompt {
                provider: ProviderSetupKind::ClaudeApi,
            });
        })];
        let deepseek_actions: Vec<SelectionAction> = vec![Box::new(|tx| {
            tx.send(AppEvent::OpenProviderLoginPrompt {
                provider: ProviderSetupKind::DeepSeek,
            });
        })];
        let kimi_actions: Vec<SelectionAction> = vec![Box::new(|tx| {
            tx.send(AppEvent::OpenProviderLoginPrompt {
                provider: ProviderSetupKind::Kimi,
            });
        })];
        let common_actions: Vec<SelectionAction> = vec![Box::new(|tx| {
            tx.send(AppEvent::OpenProviderLoginPrompt {
                provider: ProviderSetupKind::Common,
            });
        })];
        let anthropic_oauth_actions: Vec<SelectionAction> = vec![Box::new(|tx| {
            tx.send(AppEvent::BeginAnthropicOauthLogin);
        })];
        let anthropic_api_actions: Vec<SelectionAction> = vec![Box::new(|tx| {
            tx.send(AppEvent::OpenProviderLoginPrompt {
                provider: ProviderSetupKind::Anthropic,
            });
        })];

        let mut header = ColumnRenderable::new();
        header.push(Line::from("AI provider login").bold());
        header.push(Line::from(
            "Manage credentials without blocking startup when another provider is usable.".dim(),
        ));
        header.push(Line::from(
            "Tip: Praxis can import the local Claude Code Pro/Max OAuth login without exposing it."
                .dim(),
        ));

        self.bottom_pane.show_selection_view(SelectionViewParams {
            header: Box::new(header),
            footer_hint: Some(standard_popup_hint_line()),
            items: vec![
                SelectionItem {
                    name: "ChatGPT / OpenAI account".to_string(),
                    description: Some(
                        "Uses inherited ChatGPT/OpenAI credentials when present.".to_string(),
                    ),
                    actions: openai_actions,
                    dismiss_on_select: true,
                    ..Default::default()
                },
                SelectionItem {
                    name: "Codex / OpenAI API key".to_string(),
                    description: Some(
                        "Configure URL and API key; defaults to the official Responses endpoint and uses the Codex model catalog."
                            .to_string(),
                    ),
                    actions: openai_api_actions,
                    dismiss_on_select: true,
                    ..Default::default()
                },
                SelectionItem {
                    name: "Claude Pro/Max account".to_string(),
                    description: Some(
                        "Authorize Praxis with the local Claude account OAuth flow.".to_string(),
                    ),
                    actions: anthropic_oauth_actions,
                    dismiss_on_select: true,
                    ..Default::default()
                },
                SelectionItem {
                    name: "Anthropic API key".to_string(),
                    description: Some(
                        "Configure URL and API key; defaults to the official Claude endpoint and uses the Claude account model catalog."
                            .to_string(),
                    ),
                    actions: anthropic_api_actions,
                    dismiss_on_select: true,
                    ..Default::default()
                },
                SelectionItem {
                    name: "Responses API".to_string(),
                    description: Some(
                        "Configure a separate Responses endpoint and enter any model name."
                            .to_string(),
                    ),
                    actions: responses_api_actions,
                    dismiss_on_select: true,
                    ..Default::default()
                },
                SelectionItem {
                    name: "Claude API".to_string(),
                    description: Some(
                        "Configure a separate Claude Messages endpoint and enter any model name."
                            .to_string(),
                    ),
                    actions: claude_api_actions,
                    dismiss_on_select: true,
                    ..Default::default()
                },
                SelectionItem {
                    name: "DeepSeek API key".to_string(),
                    description: Some(
                        "Configure DeepSeek with the Praxis DeepSeek profile.".to_string(),
                    ),
                    actions: deepseek_actions,
                    dismiss_on_select: true,
                    ..Default::default()
                },
                SelectionItem {
                    name: "Kimi Code API key".to_string(),
                    description: Some(
                        "Configure K3 and K2.7 Code with the Praxis Kimi profile.".to_string(),
                    ),
                    actions: kimi_actions,
                    dismiss_on_select: true,
                    ..Default::default()
                },
                SelectionItem {
                    name: "Common API key".to_string(),
                    description: Some(
                        "Configure a generic OpenAI-compatible endpoint.".to_string(),
                    ),
                    actions: common_actions,
                    dismiss_on_select: true,
                    ..Default::default()
                },
            ],
            ..Default::default()
        });
        self.request_redraw();
    }

    pub(crate) fn open_provider_login_prompt(&mut self, provider: ProviderSetupKind) {
        let tx = self.app_event_tx.clone();
        let on_submit = Box::new(move |raw: String| {
            let raw = zeroize::Zeroizing::new(raw);
            match provider.parse_selection(raw.as_str()) {
                Ok(selection) => {
                    tx.send(AppEvent::ApplyProviderSetup {
                        model: selection.model,
                        provider_id: selection.provider_id,
                        provider: selection.provider,
                        effort: selection.effort,
                        api_key: selection.api_key,
                    });
                }
                Err(err) => {
                    tx.send(AppEvent::InsertHistoryCell(Box::new(
                        history_cell::new_error_event(err),
                    )));
                }
            }
        });
        let view = CustomPromptView::new_secret(
            provider.input_title(),
            provider.input_placeholder(),
            provider.input_context_label(),
            on_submit,
        );
        self.bottom_pane.show_view(Box::new(view));
        self.request_redraw();
    }

    pub(super) fn handle_login_command_args(&mut self, args: &str) {
        let trimmed = args.trim();
        if trimmed.is_empty() {
            self.open_login_popup();
            return;
        }

        let mut parts = trimmed.splitn(2, char::is_whitespace);
        let target = parts.next().unwrap_or_default();
        let rest = parts.next().unwrap_or_default().trim();
        match Self::login_provider_target(target) {
            Some(provider) if rest.is_empty() => self.open_provider_login_prompt(provider),
            Some(provider) => {
                self.add_error_message(format!(
                    "Do not place the {} API key in a slash command. Run `/login {}` and enter it in the masked prompt.",
                    provider.label(),
                    provider.provider_id()
                ));
            }
            None if target.eq_ignore_ascii_case("anthropic")
                || target.eq_ignore_ascii_case("claude")
                || target.eq_ignore_ascii_case("claude-account") =>
            {
                self.app_event_tx.send(AppEvent::BeginAnthropicOauthLogin);
            }
            None if target.eq_ignore_ascii_case("chatgpt")
                || target.eq_ignore_ascii_case("codex")
                || target.eq_ignore_ascii_case("openai") =>
            {
                self.add_info_message(
                    "Praxis uses your ChatGPT/OpenAI login when available.".to_string(),
                    Some(
                        "Use /login codex-api, /login responses-api, /login anthropic-api, or /login claude-api for API-key providers."
                            .to_string(),
                    ),
                );
            }
            None => self.add_error_message(
                "Usage: /login [chatgpt|codex-api|claude|anthropic-api|responses-api|claude-api|kimi|deepseek|common]".to_string(),
            ),
        }
        self.bottom_pane.drain_pending_submission_state();
    }

    fn login_provider_target(target: &str) -> Option<ProviderSetupKind> {
        match target.to_ascii_lowercase().as_str() {
            "codex-api" | "openai-api" => Some(ProviderSetupKind::OpenAi),
            "anthropic-api" => Some(ProviderSetupKind::Anthropic),
            "responses" | "responses-api" => Some(ProviderSetupKind::ResponsesApi),
            "claude-api" | "claude-messages" => Some(ProviderSetupKind::ClaudeApi),
            "deepseek" | "ds" => Some(ProviderSetupKind::DeepSeek),
            "kimi" | "moonshot" => Some(ProviderSetupKind::Kimi),
            "common" | "openai-compatible" | "compatible" => Some(ProviderSetupKind::Common),
            _ => None,
        }
    }
}
