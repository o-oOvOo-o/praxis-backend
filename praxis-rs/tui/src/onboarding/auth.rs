#![allow(clippy::unwrap_used)]

use crossterm::event::KeyCode;
use crossterm::event::KeyEvent;
use crossterm::event::KeyEventKind;
use crossterm::event::KeyModifiers;
use praxis_app_gateway_client::AppGatewayRequestHandle;
use praxis_app_gateway_protocol::AccountLoginCompletedNotification;
use praxis_app_gateway_protocol::AccountUpdatedNotification;
use praxis_app_gateway_protocol::AuthMode as AppGatewayAuthMode;
use praxis_app_gateway_protocol::CancelLoginAccountParams;
use praxis_app_gateway_protocol::ClientRequest;
use praxis_app_gateway_protocol::LoginAccountParams;
use praxis_app_gateway_protocol::LoginAccountResponse;
use praxis_core::config::edit::ConfigEditsBuilder;
use praxis_login::AuthCredentialsStoreMode;
use praxis_login::DeviceCode;
use ratatui::buffer::Buffer;
use ratatui::layout::Constraint;
use ratatui::layout::Layout;
use ratatui::layout::Rect;
use ratatui::prelude::Widget;
use ratatui::style::Color;
use ratatui::style::Modifier;
use ratatui::style::Style;
use ratatui::style::Stylize;
use ratatui::text::Line;
use ratatui::widgets::Block;
use ratatui::widgets::BorderType;
use ratatui::widgets::Borders;
use ratatui::widgets::Paragraph;
use ratatui::widgets::WidgetRef;
use ratatui::widgets::Wrap;

use praxis_protocol::config_types::ForcedLoginMethod;
use std::sync::Arc;
use std::sync::RwLock;
use uuid::Uuid;

use crate::LoginStatus;
use crate::custom_terminal::mark_hyperlink_cells;
use crate::onboarding::onboarding_screen::KeyboardHandler;
use crate::onboarding::onboarding_screen::StepStateProvider;
use crate::provider_setup::ProviderSetupKind;
use crate::shimmer::shimmer_spans;
use crate::tui::FrameRequester;

/// Marks buffer cells that have cyan+underlined style as an OSC 8 hyperlink.
///
/// Terminal emulators recognise the OSC 8 escape sequence and treat the entire
/// marked region as a single clickable link, regardless of row wrapping.  This
/// is necessary because ratatui's cell-based rendering emits `MoveTo` at every
/// row boundary, which breaks normal terminal URL detection for long URLs that
/// wrap across multiple rows.
pub(crate) fn mark_url_hyperlink(buf: &mut Buffer, area: Rect, url: &str) {
    mark_hyperlink_cells(buf, area, url, |cell| {
        cell.fg == Color::Cyan && cell.modifier.contains(Modifier::UNDERLINED)
    });
}
use std::path::PathBuf;
use tokio::sync::Notify;

use super::onboarding_screen::StepState;

mod headless_chatgpt_login;

#[derive(Clone)]
pub(crate) enum SignInState {
    PickMode,
    ChatGptContinueInBrowser(ContinueInBrowserState),
    #[allow(dead_code)]
    ChatGptDeviceCode(ContinueWithDeviceCodeState),
    ChatGptSuccessMessage,
    ChatGptSuccess,
    ApiKeyEntry(ApiKeyInputState),
    ApiKeyConfigured {
        provider_label: String,
    },
    ClaudeNotice,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SignInOption {
    ChatGpt,
    DeviceCode,
    DeepSeekApiKey,
    CommonApiKey,
    Anthropic,
}

const API_KEY_DISABLED_MESSAGE: &str = "API key login is disabled.";
fn onboarding_request_id() -> praxis_app_gateway_protocol::RequestId {
    praxis_app_gateway_protocol::RequestId::String(Uuid::new_v4().to_string())
}

#[derive(Clone)]
pub(crate) struct ApiKeyInputState {
    provider: ProviderSetupKind,
    active_field: ApiKeyInputField,
    api_key: String,
    base_url: String,
    model: String,
    prepopulated_from_env: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ApiKeyInputField {
    ApiKey,
    BaseUrl,
    Model,
}

impl ApiKeyInputField {
    fn next(self) -> Self {
        match self {
            Self::ApiKey => Self::BaseUrl,
            Self::BaseUrl => Self::Model,
            Self::Model => Self::ApiKey,
        }
    }

    fn previous(self) -> Self {
        match self {
            Self::ApiKey => Self::Model,
            Self::BaseUrl => Self::ApiKey,
            Self::Model => Self::BaseUrl,
        }
    }
}

impl ApiKeyInputState {
    fn new(provider: ProviderSetupKind) -> Self {
        let prefill_from_env = provider.prefilled_api_key();
        Self {
            provider,
            active_field: ApiKeyInputField::ApiKey,
            api_key: prefill_from_env.clone().unwrap_or_default(),
            base_url: provider.default_base_url().to_string(),
            model: provider.default_model().to_string(),
            prepopulated_from_env: prefill_from_env.is_some(),
        }
    }

    fn active_value_mut(&mut self) -> &mut String {
        match self.active_field {
            ApiKeyInputField::ApiKey => &mut self.api_key,
            ApiKeyInputField::BaseUrl => &mut self.base_url,
            ApiKeyInputField::Model => &mut self.model,
        }
    }
}

#[derive(Clone)]
/// Used to manage the lifecycle of SpawnedLogin and ensure it gets cleaned up.
pub(crate) struct ContinueInBrowserState {
    login_id: String,
    auth_url: String,
}

#[derive(Clone)]
pub(crate) struct ContinueWithDeviceCodeState {
    device_code: Option<DeviceCode>,
    cancel: Option<Arc<Notify>>,
}

impl KeyboardHandler for AuthModeWidget {
    fn handle_key_event(&mut self, key_event: KeyEvent) {
        if self.handle_provider_key_flow_key_event(&key_event) {
            return;
        }

        match key_event.code {
            KeyCode::Up | KeyCode::Char('k') => {
                self.move_highlight(/*delta*/ -1);
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.move_highlight(/*delta*/ 1);
            }
            KeyCode::Char(c) if c.is_ascii_digit() => {
                if let Some(index) = c
                    .to_digit(10)
                    .and_then(|digit| usize::try_from(digit).ok())
                    .and_then(|digit| digit.checked_sub(1))
                {
                    self.select_option_by_index(index);
                }
            }
            KeyCode::Enter => {
                let sign_in_state = { (*self.sign_in_state.read().unwrap()).clone() };
                match sign_in_state {
                    SignInState::PickMode => {
                        self.handle_sign_in_option(self.highlighted_mode);
                    }
                    SignInState::ChatGptSuccessMessage => {
                        *self.sign_in_state.write().unwrap() = SignInState::ChatGptSuccess;
                    }
                    _ => {}
                }
            }
            KeyCode::Esc => {
                tracing::info!("Esc pressed");
                self.cancel_active_attempt();
            }
            _ => {}
        }
    }

    fn handle_paste(&mut self, pasted: String) {
        let _ = self.handle_provider_key_entry_paste(pasted);
    }
}

#[derive(Clone)]
#[allow(dead_code)]
pub(crate) struct AuthModeWidget {
    pub request_frame: FrameRequester,
    pub highlighted_mode: SignInOption,
    pub error: Arc<RwLock<Option<String>>>,
    pub sign_in_state: Arc<RwLock<SignInState>>,
    pub praxis_home: PathBuf,
    pub cli_auth_credentials_store_mode: AuthCredentialsStoreMode,
    pub login_status: LoginStatus,
    pub app_gateway_request_handle: AppGatewayRequestHandle,
    pub forced_chatgpt_workspace_id: Option<String>,
    pub forced_login_method: Option<ForcedLoginMethod>,
    pub animations_enabled: bool,
}

impl AuthModeWidget {
    pub(crate) fn cancel_active_attempt(&self) {
        let mut sign_in_state = self.sign_in_state.write().unwrap();
        match &*sign_in_state {
            SignInState::ChatGptContinueInBrowser(state) => {
                let request_handle = self.app_gateway_request_handle.clone();
                let login_id = state.login_id.clone();
                tokio::spawn(async move {
                    let _ = request_handle
                        .request_typed::<praxis_app_gateway_protocol::CancelLoginAccountResponse>(
                            ClientRequest::CancelLoginAccount {
                                request_id: onboarding_request_id(),
                                params: CancelLoginAccountParams { login_id },
                            },
                        )
                        .await;
                });
            }
            SignInState::ChatGptDeviceCode(state) => {
                if let Some(cancel) = &state.cancel {
                    cancel.notify_one();
                }
            }
            _ => return,
        }
        *sign_in_state = SignInState::PickMode;
        drop(sign_in_state);
        self.set_error(/*message*/ None);
        self.request_frame.schedule_frame();
    }

    fn set_error(&self, message: Option<String>) {
        *self.error.write().unwrap() = message;
    }

    fn error_message(&self) -> Option<String> {
        self.error.read().unwrap().clone()
    }

    fn is_api_login_allowed(&self) -> bool {
        !matches!(self.forced_login_method, Some(ForcedLoginMethod::Chatgpt))
    }

    fn is_chatgpt_login_allowed(&self) -> bool {
        !matches!(self.forced_login_method, Some(ForcedLoginMethod::Api))
    }

    fn displayed_sign_in_options(&self) -> Vec<SignInOption> {
        let mut options = vec![SignInOption::ChatGpt];
        if self.is_chatgpt_login_allowed() {
            options.push(SignInOption::DeviceCode);
        }
        if self.is_api_login_allowed() {
            options.push(SignInOption::DeepSeekApiKey);
            options.push(SignInOption::CommonApiKey);
        }
        options.push(SignInOption::Anthropic);
        options
    }

    fn selectable_sign_in_options(&self) -> Vec<SignInOption> {
        let mut options = Vec::new();
        if self.is_chatgpt_login_allowed() {
            options.push(SignInOption::ChatGpt);
            options.push(SignInOption::DeviceCode);
        }
        if self.is_api_login_allowed() {
            options.push(SignInOption::DeepSeekApiKey);
            options.push(SignInOption::CommonApiKey);
        }
        options.push(SignInOption::Anthropic);
        options
    }

    fn move_highlight(&mut self, delta: isize) {
        let options = self.selectable_sign_in_options();
        if options.is_empty() {
            return;
        }

        let current_index = options
            .iter()
            .position(|option| *option == self.highlighted_mode)
            .unwrap_or(0);
        let next_index =
            (current_index as isize + delta).rem_euclid(options.len() as isize) as usize;
        self.highlighted_mode = options[next_index];
    }

    fn select_option_by_index(&mut self, index: usize) {
        let options = self.displayed_sign_in_options();
        if let Some(option) = options.get(index).copied() {
            self.handle_sign_in_option(option);
        }
    }

    fn handle_sign_in_option(&mut self, option: SignInOption) {
        match option {
            SignInOption::ChatGpt => {
                if self.is_chatgpt_login_allowed() {
                    self.start_chatgpt_login();
                }
            }
            SignInOption::DeviceCode => {
                if self.is_chatgpt_login_allowed() {
                    self.start_device_code_login();
                }
            }
            SignInOption::DeepSeekApiKey => {
                if self.is_api_login_allowed() {
                    self.start_provider_key_entry(ProviderSetupKind::DeepSeek);
                } else {
                    self.disallow_api_login();
                }
            }
            SignInOption::CommonApiKey => {
                if self.is_api_login_allowed() {
                    self.start_provider_key_entry(ProviderSetupKind::Common);
                } else {
                    self.disallow_api_login();
                }
            }
            SignInOption::Anthropic => {
                self.show_anthropic_notice();
            }
        }
    }

    fn disallow_api_login(&mut self) {
        self.highlighted_mode = SignInOption::ChatGpt;
        self.set_error(Some(API_KEY_DISABLED_MESSAGE.to_string()));
        *self.sign_in_state.write().unwrap() = SignInState::PickMode;
        self.request_frame.schedule_frame();
    }

    fn render_pick_mode(&self, area: Rect, buf: &mut Buffer) {
        let mut lines: Vec<Line> = vec![
            Line::from(vec![
                "  ".into(),
                "Sign in with ChatGPT to use Praxis as part of your paid plan".into(),
            ]),
            Line::from(vec![
                "  ".into(),
                "or configure a Praxis provider key for specialized/common models".into(),
            ]),
            "".into(),
        ];

        let create_mode_item = |idx: usize,
                                selected_mode: SignInOption,
                                text: &str,
                                description: &str|
         -> Vec<Line<'static>> {
            let is_selected = self.highlighted_mode == selected_mode;
            let caret = if is_selected { ">" } else { " " };

            let line1 = if is_selected {
                Line::from(vec![
                    format!("{caret} {index}. ", index = idx + 1).cyan().dim(),
                    text.to_string().cyan(),
                ])
            } else {
                format!("  {index}. {text}", index = idx + 1).into()
            };

            let line2 = if is_selected {
                Line::from(format!("     {description}"))
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::DIM)
            } else {
                Line::from(format!("     {description}"))
                    .style(Style::default().add_modifier(Modifier::DIM))
            };

            vec![line1, line2]
        };

        let chatgpt_description = if !self.is_chatgpt_login_allowed() {
            "ChatGPT login is disabled"
        } else {
            "Uses ChatGPT/OpenAI login when available; otherwise starts Praxis login"
        };
        let device_code_description = "Sign in from another device with a one-time code";

        for (idx, option) in self.displayed_sign_in_options().into_iter().enumerate() {
            match option {
                SignInOption::ChatGpt => {
                    lines.extend(create_mode_item(
                        idx,
                        option,
                        "Sign in with ChatGPT",
                        chatgpt_description,
                    ));
                }
                SignInOption::DeviceCode => {
                    lines.extend(create_mode_item(
                        idx,
                        option,
                        "Sign in with Device Code",
                        device_code_description,
                    ));
                }
                SignInOption::DeepSeekApiKey => {
                    lines.extend(create_mode_item(
                        idx,
                        option,
                        "Sign in with DeepSeek API key",
                        "Use Praxis DeepSeek profile with deepseek-v4-pro",
                    ));
                }
                SignInOption::CommonApiKey => {
                    lines.extend(create_mode_item(
                        idx,
                        option,
                        "Sign in with Common API key",
                        "Use a generic OpenAI-compatible endpoint",
                    ));
                }
                SignInOption::Anthropic => {
                    lines.extend(create_mode_item(
                        idx,
                        option,
                        "Sign in with Anthropic",
                        "Connect your Anthropic account",
                    ));
                }
            }
            lines.push("".into());
        }

        if !self.is_api_login_allowed() {
            lines.push(
                "  API key login is disabled by this workspace. Sign in with ChatGPT to continue."
                    .dim()
                    .into(),
            );
            lines.push("".into());
        }
        lines.push(
            // AE: Following styles.md, this should probably be Cyan because it's a user input tip.
            //     But leaving this for a future cleanup.
            "  Press Enter to continue".dim().into(),
        );
        if let Some(err) = self.error_message() {
            lines.push("".into());
            lines.push(err.red().into());
        }

        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .render(area, buf);
    }

    fn render_continue_in_browser(&self, area: Rect, buf: &mut Buffer) {
        let mut spans = vec!["  ".into()];
        if self.animations_enabled {
            // Schedule a follow-up frame to keep the shimmer animation going.
            self.request_frame
                .schedule_frame_in(std::time::Duration::from_millis(100));
            spans.extend(shimmer_spans("Finish signing in via your browser"));
        } else {
            spans.push("Finish signing in via your browser".into());
        }
        let mut lines = vec![spans.into(), "".into()];

        let sign_in_state = self.sign_in_state.read().unwrap();
        let auth_url = if let SignInState::ChatGptContinueInBrowser(state) = &*sign_in_state
            && !state.auth_url.is_empty()
        {
            lines.push("  If the link doesn't open automatically, open the following link to authenticate:".into());
            lines.push("".into());
            lines.push(Line::from(vec![
                "  ".into(),
                state.auth_url.as_str().cyan().underlined(),
            ]));
            lines.push("".into());
            lines.push(Line::from(vec![
                "  On a remote or headless machine? Press Esc and choose ".into(),
                "Sign in with Device Code".cyan(),
                ".".into(),
            ]));
            lines.push("".into());
            Some(state.auth_url.clone())
        } else {
            None
        };

        lines.push("  Press Esc to cancel".dim().into());
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .render(area, buf);

        // Wrap cyan+underlined URL cells with OSC 8 so the terminal treats
        // the entire region as a single clickable hyperlink.
        if let Some(url) = &auth_url {
            mark_url_hyperlink(buf, area, url);
        }
    }

    fn render_chatgpt_success_message(&self, area: Rect, buf: &mut Buffer) {
        let lines = vec![
            "✓ Signed in with your ChatGPT account".fg(Color::Green).into(),
            "".into(),
            "  Before you start:".into(),
            "".into(),
            "  Decide how much autonomy you want to grant Praxis".into(),
            Line::from(vec![
                "  For more details see the ".into(),
                "\u{1b}]8;;https://github.com/o-oOvOo-o/praxis-backend/blob/main/docs/sandbox.md\u{7}Praxis docs\u{1b}]8;;\u{7}".underlined(),
            ])
            .dim(),
            "".into(),
            "  Praxis can make mistakes".into(),
            "  Review the code it writes and commands it runs".dim().into(),
            "".into(),
            "  Powered by your ChatGPT account".into(),
            Line::from(vec![
                "  Uses your plan's rate limits and ".into(),
                "\u{1b}]8;;https://chatgpt.com/#settings\u{7}training data preferences\u{1b}]8;;\u{7}".underlined(),
            ])
            .dim(),
            "".into(),
            "  Press Enter to continue".fg(Color::Cyan).into(),
        ];

        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .render(area, buf);
    }

    fn render_chatgpt_success(&self, area: Rect, buf: &mut Buffer) {
        let lines = vec![
            "✓ Signed in with your ChatGPT account"
                .fg(Color::Green)
                .into(),
        ];

        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .render(area, buf);
    }

    fn render_api_key_configured(&self, area: Rect, buf: &mut Buffer, provider_label: &str) {
        let lines = vec![
            Line::from(vec![
                "✓ ".fg(Color::Green),
                provider_label.to_string().fg(Color::Green),
                " provider configured".fg(Color::Green),
            ]),
            "".into(),
            "  Praxis saved this provider under model_providers.".into(),
            "  ChatGPT/OpenAI auth remains available when configured."
                .dim()
                .into(),
        ];

        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .render(area, buf);
    }

    fn render_api_key_entry(&self, area: Rect, buf: &mut Buffer, state: &ApiKeyInputState) {
        let [
            intro_area,
            api_key_area,
            base_url_area,
            model_area,
            footer_area,
        ] = Layout::vertical([
            Constraint::Min(4),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Min(2),
        ])
        .areas(area);

        let mut intro_lines: Vec<Line> = vec![
            Line::from(vec![
                "> ".into(),
                format!("Configure {}", state.provider.label()).bold(),
            ]),
            "".into(),
            "  This writes a Praxis model provider and switches the active model to it.".into(),
            "  It does not write auth.json or replace ChatGPT/OpenAI credentials."
                .dim()
                .into(),
            "".into(),
        ];
        if state.prepopulated_from_env {
            let env_key = state.provider.env_key().unwrap_or("provider API key");
            intro_lines.push(format!("  Detected {env_key} environment variable.").into());
            intro_lines.push(
                "  Paste a different key if you prefer to use another account."
                    .dim()
                    .into(),
            );
            intro_lines.push("".into());
        }
        Paragraph::new(intro_lines)
            .wrap(Wrap { trim: false })
            .render(intro_area, buf);

        self.render_api_key_field(
            api_key_area,
            buf,
            "API key",
            &state.api_key,
            "Paste or type the provider API key",
            state.active_field == ApiKeyInputField::ApiKey,
        );
        self.render_api_key_field(
            base_url_area,
            buf,
            "Base URL",
            &state.base_url,
            "https://api.example.com",
            state.active_field == ApiKeyInputField::BaseUrl,
        );
        self.render_api_key_field(
            model_area,
            buf,
            "Model",
            &state.model,
            "model-name",
            state.active_field == ApiKeyInputField::Model,
        );

        let mut footer_lines: Vec<Line> = vec![
            "  Tab/Shift+Tab changes field".dim().into(),
            "  Press Enter to save".dim().into(),
            "  Press Esc to go back".dim().into(),
        ];
        if let Some(error) = self.error_message() {
            footer_lines.push("".into());
            footer_lines.push(error.red().into());
        }
        Paragraph::new(footer_lines)
            .wrap(Wrap { trim: false })
            .render(footer_area, buf);
    }

    fn render_api_key_field(
        &self,
        area: Rect,
        buf: &mut Buffer,
        title: &'static str,
        value: &str,
        placeholder: &'static str,
        active: bool,
    ) {
        let border_style = if active {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        let content_line: Line = if value.is_empty() {
            vec![placeholder.dim()].into()
        } else {
            Line::from(value.to_string())
        };
        Paragraph::new(content_line)
            .wrap(Wrap { trim: false })
            .block(
                Block::default()
                    .title(title)
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(border_style),
            )
            .render(area, buf);
    }

    fn render_claude_notice(&self, area: Rect, buf: &mut Buffer) {
        let lines: Vec<Line> = vec![
            Line::from(vec![
                "> ".into(),
                "Claude Placeholder / Anthropic Statement".bold(),
            ]),
            "".into(),
            "  This is a placeholder, not a Claude adapter.".into(),
            "  Praxis is not rewarding Anthropic with a first-class route here.".into(),
            "".into(),
            "  Anthropic has built a public moral posture around opposition to distillation,"
                .into(),
            "  while benefiting from the same open research, shared engineering practice,".into(),
            "  and industry-wide iteration that made modern agent systems possible.".into(),
            "".into(),
            "  Praxis will not treat that contradiction as a first-class integration target."
                .into(),
            "  Adapter work is reserved for model systems with clear interfaces, reliable".into(),
            "  behavior, and product direction that materially strengthens users and agents."
                .into(),
            "".into(),
            "  Press Esc to go back".dim().into(),
        ];

        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .render(area, buf);
    }

    fn handle_provider_key_flow_key_event(&mut self, key_event: &KeyEvent) -> bool {
        let mut should_save: Option<ApiKeyInputState> = None;
        let mut should_request_frame = false;

        {
            let mut guard = self.sign_in_state.write().unwrap();
            match &mut *guard {
                SignInState::ApiKeyEntry(state) => {
                    match key_event.code {
                        KeyCode::Esc => {
                            *guard = SignInState::PickMode;
                            self.set_error(/*message*/ None);
                            should_request_frame = true;
                        }
                        KeyCode::Tab => {
                            state.active_field = state.active_field.next();
                            should_request_frame = true;
                        }
                        KeyCode::BackTab => {
                            state.active_field = state.active_field.previous();
                            should_request_frame = true;
                        }
                        KeyCode::Enter => {
                            if let Some(message) = self.validate_provider_key_state(state) {
                                self.set_error(Some(message));
                                should_request_frame = true;
                            } else {
                                should_save = Some(state.clone());
                            }
                        }
                        KeyCode::Backspace => {
                            if state.active_field == ApiKeyInputField::ApiKey
                                && state.prepopulated_from_env
                            {
                                state.api_key.clear();
                                state.prepopulated_from_env = false;
                            } else {
                                state.active_value_mut().pop();
                            }
                            self.set_error(/*message*/ None);
                            should_request_frame = true;
                        }
                        KeyCode::Char(c)
                            if key_event.kind == KeyEventKind::Press
                                && !key_event.modifiers.contains(KeyModifiers::SUPER)
                                && !key_event.modifiers.contains(KeyModifiers::CONTROL)
                                && !key_event.modifiers.contains(KeyModifiers::ALT) =>
                        {
                            if state.active_field == ApiKeyInputField::ApiKey
                                && state.prepopulated_from_env
                            {
                                state.api_key.clear();
                                state.prepopulated_from_env = false;
                            }
                            state.active_value_mut().push(c);
                            self.set_error(/*message*/ None);
                            should_request_frame = true;
                        }
                        _ => {}
                    }
                }
                SignInState::ClaudeNotice => match key_event.code {
                    KeyCode::Esc | KeyCode::Enter => {
                        *guard = SignInState::PickMode;
                        self.set_error(/*message*/ None);
                        should_request_frame = true;
                    }
                    _ => {}
                },
                _ => return false,
            }
        }

        if let Some(state) = should_save {
            self.save_provider_key(state);
        } else if should_request_frame {
            self.request_frame.schedule_frame();
        }
        true
    }

    fn handle_provider_key_entry_paste(&mut self, pasted: String) -> bool {
        let trimmed = pasted.trim();
        if trimmed.is_empty() {
            return false;
        }

        let mut guard = self.sign_in_state.write().unwrap();
        if let SignInState::ApiKeyEntry(state) = &mut *guard {
            if state.active_field == ApiKeyInputField::ApiKey && state.prepopulated_from_env {
                state.api_key = trimmed.to_string();
                state.prepopulated_from_env = false;
            } else {
                state.active_value_mut().push_str(trimmed);
            }
            self.set_error(/*message*/ None);
        } else {
            return false;
        }

        drop(guard);
        self.request_frame.schedule_frame();
        true
    }

    fn start_provider_key_entry(&mut self, provider: ProviderSetupKind) {
        if !self.is_api_login_allowed() {
            self.disallow_api_login();
            return;
        }
        self.set_error(/*message*/ None);
        *self.sign_in_state.write().unwrap() =
            SignInState::ApiKeyEntry(ApiKeyInputState::new(provider));
        self.request_frame.schedule_frame();
    }

    fn show_anthropic_notice(&mut self) {
        self.set_error(/*message*/ None);
        *self.sign_in_state.write().unwrap() = SignInState::ClaudeNotice;
        self.request_frame.schedule_frame();
    }

    fn validate_provider_key_state(&self, state: &ApiKeyInputState) -> Option<String> {
        if state.api_key.trim().is_empty() {
            return Some("API key cannot be empty".to_string());
        }
        if state.base_url.trim().is_empty() {
            return Some("Base URL cannot be empty".to_string());
        }
        if state.model.trim().is_empty() {
            return Some("Model cannot be empty".to_string());
        }
        None
    }

    fn normalize_common_base_url(&self, raw: &str) -> String {
        raw.trim().trim_end_matches('/').to_string()
    }

    fn save_provider_key(&mut self, state: ApiKeyInputState) {
        if !self.is_api_login_allowed() {
            self.disallow_api_login();
            return;
        }
        self.set_error(/*message*/ None);
        let praxis_home = self.praxis_home.clone();
        let sign_in_state = self.sign_in_state.clone();
        let error = self.error.clone();
        let request_frame = self.request_frame.clone();
        let api_key = state.api_key.trim().to_string();
        let base_url = self.normalize_common_base_url(&state.base_url);
        let model = state.model.trim().to_string();
        let provider = state.provider.build_provider(api_key, base_url);
        let provider_id = state.provider.provider_id().to_string();
        let provider_label = state.provider.label().to_string();
        let default_effort = state.provider.default_effort();
        let retry_state = state.clone();

        tokio::spawn(async move {
            let result = ConfigEditsBuilder::new(&praxis_home)
                .upsert_model_provider(provider_id.as_str(), &provider)
                .set_model_provider(Some(provider_id.as_str()))
                .set_model(Some(model.as_str()), default_effort)
                .apply()
                .await;

            match result {
                Ok(()) => {
                    *error.write().unwrap() = None;
                    *sign_in_state.write().unwrap() =
                        SignInState::ApiKeyConfigured { provider_label };
                }
                Err(err) => {
                    *error.write().unwrap() =
                        Some(format!("Failed to save Praxis provider key: {err}"));
                    *sign_in_state.write().unwrap() = SignInState::ApiKeyEntry(retry_state);
                }
            }
            request_frame.schedule_frame();
        });
        self.request_frame.schedule_frame();
    }

    fn handle_existing_chatgpt_login(&mut self) -> bool {
        if matches!(
            self.login_status,
            LoginStatus::AuthMode(AppGatewayAuthMode::Chatgpt)
                | LoginStatus::AuthMode(AppGatewayAuthMode::ChatgptAuthTokens)
        ) {
            *self.sign_in_state.write().unwrap() = SignInState::ChatGptSuccess;
            self.request_frame.schedule_frame();
            true
        } else {
            false
        }
    }

    /// Kicks off the ChatGPT auth flow and keeps the UI state consistent with the attempt.
    fn start_chatgpt_login(&mut self) {
        // If we're already authenticated with ChatGPT, don't start a new login –
        // just proceed to the success message flow.
        if self.handle_existing_chatgpt_login() {
            return;
        }

        self.set_error(/*message*/ None);
        let request_handle = self.app_gateway_request_handle.clone();
        let sign_in_state = self.sign_in_state.clone();
        let error = self.error.clone();
        let request_frame = self.request_frame.clone();
        tokio::spawn(async move {
            match request_handle
                .request_typed::<LoginAccountResponse>(ClientRequest::LoginAccount {
                    request_id: onboarding_request_id(),
                    params: LoginAccountParams::Chatgpt,
                })
                .await
            {
                Ok(LoginAccountResponse::Chatgpt { login_id, auth_url }) => {
                    maybe_open_auth_url_in_browser(&request_handle, &auth_url);
                    *error.write().unwrap() = None;
                    *sign_in_state.write().unwrap() =
                        SignInState::ChatGptContinueInBrowser(ContinueInBrowserState {
                            login_id,
                            auth_url,
                        });
                }
                Ok(other) => {
                    *sign_in_state.write().unwrap() = SignInState::PickMode;
                    *error.write().unwrap() = Some(format!(
                        "Unexpected account/login/start response: {other:?}"
                    ));
                }
                Err(err) => {
                    *sign_in_state.write().unwrap() = SignInState::PickMode;
                    *error.write().unwrap() = Some(err.to_string());
                }
            }
            request_frame.schedule_frame();
        });
    }

    fn start_device_code_login(&mut self) {
        if self.handle_existing_chatgpt_login() {
            return;
        }

        self.set_error(/*message*/ None);
        headless_chatgpt_login::start_headless_chatgpt_login(self);
    }

    pub(crate) fn on_account_login_completed(
        &mut self,
        notification: AccountLoginCompletedNotification,
    ) {
        let Some(login_id) = notification.login_id else {
            return;
        };
        let guard = self.sign_in_state.read().unwrap();
        let is_matching_login = matches!(
            &*guard,
            SignInState::ChatGptContinueInBrowser(state) if state.login_id == login_id
        );
        drop(guard);
        if !is_matching_login {
            return;
        }

        if notification.success {
            self.set_error(/*message*/ None);
            *self.sign_in_state.write().unwrap() = SignInState::ChatGptSuccessMessage;
        } else {
            self.set_error(notification.error);
            *self.sign_in_state.write().unwrap() = SignInState::PickMode;
        }
        self.request_frame.schedule_frame();
    }

    pub(crate) fn on_account_updated(&mut self, notification: AccountUpdatedNotification) {
        self.login_status = notification
            .auth_mode
            .map(LoginStatus::AuthMode)
            .unwrap_or(LoginStatus::NotAuthenticated);
    }
}

impl StepStateProvider for AuthModeWidget {
    fn get_step_state(&self) -> StepState {
        let sign_in_state = self.sign_in_state.read().unwrap();
        match &*sign_in_state {
            SignInState::PickMode
            | SignInState::ApiKeyEntry(_)
            | SignInState::ClaudeNotice
            | SignInState::ChatGptContinueInBrowser(_)
            | SignInState::ChatGptDeviceCode(_)
            | SignInState::ChatGptSuccessMessage => StepState::InProgress,
            SignInState::ChatGptSuccess | SignInState::ApiKeyConfigured { .. } => {
                StepState::Complete
            }
        }
    }
}

impl WidgetRef for AuthModeWidget {
    fn render_ref(&self, area: Rect, buf: &mut Buffer) {
        let sign_in_state = self.sign_in_state.read().unwrap();
        match &*sign_in_state {
            SignInState::PickMode => {
                self.render_pick_mode(area, buf);
            }
            SignInState::ChatGptContinueInBrowser(_) => {
                self.render_continue_in_browser(area, buf);
            }
            SignInState::ChatGptDeviceCode(state) => {
                headless_chatgpt_login::render_device_code_login(self, area, buf, state);
            }
            SignInState::ChatGptSuccessMessage => {
                self.render_chatgpt_success_message(area, buf);
            }
            SignInState::ChatGptSuccess => {
                self.render_chatgpt_success(area, buf);
            }
            SignInState::ApiKeyEntry(state) => {
                self.render_api_key_entry(area, buf, state);
            }
            SignInState::ApiKeyConfigured { provider_label } => {
                self.render_api_key_configured(area, buf, provider_label);
            }
            SignInState::ClaudeNotice => {
                self.render_claude_notice(area, buf);
            }
        }
    }
}

pub(super) fn maybe_open_auth_url_in_browser(request_handle: &AppGatewayRequestHandle, url: &str) {
    if !matches!(request_handle, AppGatewayRequestHandle::Native(_)) {
        return;
    }

    if let Err(err) = webbrowser::open(url) {
        tracing::warn!("failed to open browser for login URL: {err}");
    }
}

#[cfg(test)]
mod tests;
