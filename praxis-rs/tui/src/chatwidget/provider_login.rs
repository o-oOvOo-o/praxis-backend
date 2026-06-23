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
        let deepseek_actions: Vec<SelectionAction> = vec![Box::new(|tx| {
            tx.send(AppEvent::OpenProviderLoginPrompt {
                provider: ProviderSetupKind::DeepSeek,
            });
        })];
        let common_actions: Vec<SelectionAction> = vec![Box::new(|tx| {
            tx.send(AppEvent::OpenProviderLoginPrompt {
                provider: ProviderSetupKind::Common,
            });
        })];
        let anthropic_actions: Vec<SelectionAction> = vec![Box::new(|tx| {
            tx.send(AppEvent::ShowAnthropicLoginStatement);
        })];

        let mut header = ColumnRenderable::new();
        header.push(Line::from("AI provider login").bold());
        header.push(Line::from(
            "Manage credentials without blocking startup when another provider is usable.".dim(),
        ));
        header.push(Line::from(
            "Tip: /login deepseek <api_key> or /login common api_key=... base_url=... model=..."
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
                    name: "DeepSeek API key".to_string(),
                    description: Some(
                        "Configure DeepSeek with the Praxis DeepSeek profile.".to_string(),
                    ),
                    actions: deepseek_actions,
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
                SelectionItem {
                    name: "Anthropic account".to_string(),
                    description: Some("Show the Praxis integration statement.".to_string()),
                    actions: anthropic_actions,
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
        let view = CustomPromptView::new(
            provider.input_title(),
            provider.input_placeholder(),
            provider.input_context_label(),
            Box::new(move |raw: String| match provider.parse_selection(&raw) {
                Ok(selection) => {
                    tx.send(AppEvent::ApplyProviderSetup {
                        model: selection.model,
                        provider_id: selection.provider_id,
                        provider: selection.provider,
                        effort: selection.effort,
                    });
                }
                Err(err) => {
                    tx.send(AppEvent::InsertHistoryCell(Box::new(
                        history_cell::new_error_event(err),
                    )));
                }
            }),
        );
        self.bottom_pane.show_view(Box::new(view));
        self.request_redraw();
    }

    pub(crate) fn show_anthropic_login_statement(&mut self) {
        self.add_plain_history_lines(vec![
            Line::from("Anthropic account integration").bold(),
            Line::from(""),
            Line::from(
                "Praxis does not provide Anthropic account integration. The company publicly objects to model distillation while benefiting from the same industry-wide extraction dynamics it condemns.",
            ),
            Line::from(
                "We will not spend the Praxis login surface normalizing that posture. Use ChatGPT/OpenAI, DeepSeek, or a common OpenAI-compatible endpoint instead.",
            )
            .dim(),
        ]);
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
            Some(provider) => self.submit_provider_setup_from_raw(provider, rest),
            None if target.eq_ignore_ascii_case("anthropic")
                || target.eq_ignore_ascii_case("claude") =>
            {
                self.show_anthropic_login_statement();
            }
            None if target.eq_ignore_ascii_case("chatgpt")
                || target.eq_ignore_ascii_case("codex")
                || target.eq_ignore_ascii_case("openai") =>
            {
                self.add_info_message(
                    "Praxis uses your ChatGPT/OpenAI login when available.".to_string(),
                    Some(
                        "Use /login deepseek or /login common to configure API providers."
                            .to_string(),
                    ),
                );
            }
            None => self
                .add_error_message("Usage: /login [deepseek|common|chatgpt|anthropic]".to_string()),
        }
        self.bottom_pane.drain_pending_submission_state();
    }

    fn login_provider_target(target: &str) -> Option<ProviderSetupKind> {
        match target.to_ascii_lowercase().as_str() {
            "deepseek" | "ds" => Some(ProviderSetupKind::DeepSeek),
            "common" | "openai-compatible" | "compatible" => Some(ProviderSetupKind::Common),
            _ => None,
        }
    }

    fn submit_provider_setup_from_raw(&mut self, provider: ProviderSetupKind, raw: &str) {
        match provider.parse_selection(raw) {
            Ok(selection) => {
                self.app_event_tx.send(AppEvent::ApplyProviderSetup {
                    model: selection.model,
                    provider_id: selection.provider_id,
                    provider: selection.provider,
                    effort: selection.effort,
                });
            }
            Err(err) => self.add_error_message(err),
        }
    }
}
