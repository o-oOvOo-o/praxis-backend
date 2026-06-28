use praxis_config::types::ApprovalsReviewer;
#[cfg(target_os = "windows")]
use praxis_core::windows_sandbox::WindowsSandboxLevelExt;
#[cfg(target_os = "windows")]
use praxis_protocol::config_types::WindowsSandboxLevel;
#[cfg(target_os = "windows")]
use praxis_protocol::protocol::SandboxPolicy;
use praxis_utils_approval_presets::ApprovalPreset;
#[cfg(target_os = "windows")]
use praxis_utils_approval_presets::builtin_approval_presets;
use ratatui::style::Color;
use ratatui::style::Stylize;
use ratatui::text::Line;
use ratatui::widgets::Paragraph;
use ratatui::widgets::Wrap;

use super::ChatWidget;
use crate::app_event::AppEvent;
#[cfg(target_os = "windows")]
use crate::app_event::ExitMode;
#[cfg(target_os = "windows")]
use crate::app_event::WindowsSandboxEnableMode;
use crate::bottom_pane::SelectionAction;
use crate::bottom_pane::SelectionItem;
use crate::bottom_pane::SelectionViewParams;
use crate::bottom_pane::popup_consts::standard_popup_hint_line;
use crate::render::renderable::ColumnRenderable;
use crate::render::renderable::Renderable;
#[cfg(target_os = "windows")]
use crate::status_indicator_widget::STATUS_DETAILS_DEFAULT_MAX_LINES;
#[cfg(target_os = "windows")]
use crate::status_indicator_widget::StatusDetailsCapitalization;

impl ChatWidget {
    pub(crate) fn open_full_access_confirmation(
        &mut self,
        preset: ApprovalPreset,
        return_to_permissions: bool,
    ) {
        let selected_name = preset.label.to_string();
        let approval = preset.approval;
        let sandbox = preset.sandbox;
        let mut header_children: Vec<Box<dyn Renderable>> = Vec::new();
        let title_line = Line::from("Enable full access?").bold();
        let info_line = Line::from(vec![
            "When Praxis runs with full access, it can edit any file on your computer and run commands with network, without your approval. "
                .into(),
            "Exercise caution when enabling full access. This significantly increases the risk of data loss, leaks, or unexpected behavior."
                .fg(Color::Red),
        ]);
        header_children.push(Box::new(title_line));
        header_children.push(Box::new(
            Paragraph::new(vec![info_line]).wrap(Wrap { trim: false }),
        ));
        let header = ColumnRenderable::with(header_children);

        let mut accept_actions = Self::approval_preset_actions(
            approval,
            sandbox.clone(),
            selected_name.clone(),
            ApprovalsReviewer::User,
        );
        accept_actions.push(Box::new(|tx| {
            tx.send(AppEvent::UpdateFullAccessWarningAcknowledged(true));
        }));

        let mut accept_and_remember_actions = Self::approval_preset_actions(
            approval,
            sandbox,
            selected_name,
            ApprovalsReviewer::User,
        );
        accept_and_remember_actions.push(Box::new(|tx| {
            tx.send(AppEvent::UpdateFullAccessWarningAcknowledged(true));
            tx.send(AppEvent::PersistFullAccessWarningAcknowledged);
        }));

        let deny_actions: Vec<SelectionAction> = vec![Box::new(move |tx| {
            if return_to_permissions {
                tx.send(AppEvent::OpenPermissionsPopup);
            } else {
                tx.send(AppEvent::OpenApprovalsPopup);
            }
        })];

        let items = vec![
            SelectionItem {
                name: "Yes, continue anyway".to_string(),
                description: Some("Apply full access for this session".to_string()),
                actions: accept_actions,
                dismiss_on_select: true,
                ..Default::default()
            },
            SelectionItem {
                name: "Yes, and don't ask again".to_string(),
                description: Some("Enable full access and remember this choice".to_string()),
                actions: accept_and_remember_actions,
                dismiss_on_select: true,
                ..Default::default()
            },
            SelectionItem {
                name: "Cancel".to_string(),
                description: Some("Go back without enabling full access".to_string()),
                actions: deny_actions,
                dismiss_on_select: true,
                ..Default::default()
            },
        ];

        self.bottom_pane.show_selection_view(SelectionViewParams {
            footer_hint: Some(standard_popup_hint_line()),
            items,
            header: Box::new(header),
            ..Default::default()
        });
    }

    #[cfg(target_os = "windows")]
    pub(crate) fn open_world_writable_warning_confirmation(
        &mut self,
        preset: Option<ApprovalPreset>,
        sample_paths: Vec<String>,
        extra_count: usize,
        failed_scan: bool,
    ) {
        let (approval, sandbox) = match &preset {
            Some(p) => (Some(p.approval), Some(p.sandbox.clone())),
            None => (None, None),
        };
        let mut header_children: Vec<Box<dyn Renderable>> = Vec::new();
        let describe_policy = |policy: &SandboxPolicy| match policy {
            SandboxPolicy::WorkspaceWrite { .. } => "Agent mode",
            SandboxPolicy::ReadOnly { .. } => "Read-Only mode",
            _ => "Agent mode",
        };
        let mode_label = preset
            .as_ref()
            .map(|p| describe_policy(&p.sandbox))
            .unwrap_or_else(|| describe_policy(self.config.permissions.sandbox_policy.get()));
        let info_line = if failed_scan {
            Line::from(vec![
                "We couldn't complete the world-writable scan, so protections cannot be verified. "
                    .into(),
                format!("The Windows sandbox cannot guarantee protection in {mode_label}.")
                    .fg(Color::Red),
            ])
        } else {
            Line::from(vec![
                "The Windows sandbox cannot protect writes to folders that are writable by Everyone.".into(),
                " Consider removing write access for Everyone from the following folders:".into(),
            ])
        };
        header_children.push(Box::new(
            Paragraph::new(vec![info_line]).wrap(Wrap { trim: false }),
        ));

        if !sample_paths.is_empty() {
            let mut lines: Vec<Line> = Vec::new();
            lines.push(Line::from(""));
            for p in &sample_paths {
                lines.push(Line::from(format!("  - {p}")));
            }
            if extra_count > 0 {
                lines.push(Line::from(format!("and {extra_count} more")));
            }
            header_children.push(Box::new(Paragraph::new(lines).wrap(Wrap { trim: false })));
        }
        let header = ColumnRenderable::with(header_children);

        let mut accept_actions: Vec<SelectionAction> = Vec::new();
        if preset.is_some() {
            accept_actions.push(Box::new(|tx| {
                tx.send(AppEvent::SkipNextWorldWritableScan);
            }));
        }
        if let (Some(approval), Some(sandbox)) = (approval, sandbox.clone()) {
            accept_actions.extend(Self::approval_preset_actions(
                approval,
                sandbox,
                mode_label.to_string(),
                ApprovalsReviewer::User,
            ));
        }

        let mut accept_and_remember_actions: Vec<SelectionAction> = Vec::new();
        accept_and_remember_actions.push(Box::new(|tx| {
            tx.send(AppEvent::UpdateWorldWritableWarningAcknowledged(true));
            tx.send(AppEvent::PersistWorldWritableWarningAcknowledged);
        }));
        if let (Some(approval), Some(sandbox)) = (approval, sandbox) {
            accept_and_remember_actions.extend(Self::approval_preset_actions(
                approval,
                sandbox,
                mode_label.to_string(),
                ApprovalsReviewer::User,
            ));
        }

        let items = vec![
            SelectionItem {
                name: "Continue".to_string(),
                description: Some(format!("Apply {mode_label} for this session")),
                actions: accept_actions,
                dismiss_on_select: true,
                ..Default::default()
            },
            SelectionItem {
                name: "Continue and don't warn again".to_string(),
                description: Some(format!("Enable {mode_label} and remember this choice")),
                actions: accept_and_remember_actions,
                dismiss_on_select: true,
                ..Default::default()
            },
        ];

        self.bottom_pane.show_selection_view(SelectionViewParams {
            footer_hint: Some(standard_popup_hint_line()),
            items,
            header: Box::new(header),
            ..Default::default()
        });
    }

    #[cfg(not(target_os = "windows"))]
    pub(crate) fn open_world_writable_warning_confirmation(
        &mut self,
        _preset: Option<ApprovalPreset>,
        _sample_paths: Vec<String>,
        _extra_count: usize,
        _failed_scan: bool,
    ) {
    }

    #[cfg(target_os = "windows")]
    pub(crate) fn open_windows_sandbox_enable_prompt(&mut self, preset: ApprovalPreset) {
        use ratatui_macros::line;

        if !praxis_core::windows_sandbox::ELEVATED_SANDBOX_NUX_ENABLED {
            let mut header = ColumnRenderable::new();
            header.push(*Box::new(
                Paragraph::new(vec![
                    line!["Agent mode on Windows uses an experimental sandbox to limit network and filesystem access.".bold()],
                    line!["Learn more: https://github.com/o-oOvOo-o/praxis-backend/blob/main/docs/sandbox.md"],
                ])
                .wrap(Wrap { trim: false }),
            ));

            let preset_clone = preset;
            let items = vec![
                SelectionItem {
                    name: "Enable experimental sandbox".to_string(),
                    description: None,
                    actions: vec![Box::new(move |tx| {
                        tx.send(AppEvent::EnableWindowsSandboxForAgentMode {
                            preset: preset_clone.clone(),
                            mode: WindowsSandboxEnableMode::NonAdmin,
                        });
                    })],
                    dismiss_on_select: true,
                    ..Default::default()
                },
                SelectionItem {
                    name: "Go back".to_string(),
                    description: None,
                    actions: vec![Box::new(|tx| {
                        tx.send(AppEvent::OpenApprovalsPopup);
                    })],
                    dismiss_on_select: true,
                    ..Default::default()
                },
            ];

            self.bottom_pane.show_selection_view(SelectionViewParams {
                title: None,
                footer_hint: Some(standard_popup_hint_line()),
                items,
                header: Box::new(header),
                ..Default::default()
            });
            return;
        }

        self.session_telemetry.counter(
            "praxis.windows_sandbox.elevated_prompt_shown",
            /*inc*/ 1,
            &[],
        );

        let mut header = ColumnRenderable::new();
        header.push(*Box::new(
            Paragraph::new(vec![
                line!["Set up the Praxis agent sandbox to protect your files and control network access. Learn more <https://github.com/o-oOvOo-o/praxis-backend/blob/main/docs/sandbox.md>"],
            ])
            .wrap(Wrap { trim: false }),
        ));

        let accept_otel = self.session_telemetry.clone();
        let non_admin_otel = self.session_telemetry.clone();
        let non_admin_preset = preset.clone();
        let quit_otel = self.session_telemetry.clone();
        let items = vec![
            SelectionItem {
                name: "Set up default sandbox (requires Administrator permissions)".to_string(),
                description: None,
                actions: vec![Box::new(move |tx| {
                    accept_otel.counter(
                        "praxis.windows_sandbox.elevated_prompt_accept",
                        /*inc*/ 1,
                        &[],
                    );
                    tx.send(AppEvent::BeginWindowsSandboxElevatedSetup {
                        preset: preset.clone(),
                    });
                })],
                dismiss_on_select: true,
                ..Default::default()
            },
            SelectionItem {
                name: "Use non-admin sandbox (higher risk if prompt injected)".to_string(),
                description: None,
                actions: vec![Box::new(move |tx| {
                    non_admin_otel.counter(
                        "praxis.windows_sandbox.elevated_prompt_use_non_admin",
                        /*inc*/ 1,
                        &[],
                    );
                    tx.send(AppEvent::BeginWindowsSandboxNonAdminSetup {
                        preset: non_admin_preset.clone(),
                    });
                })],
                dismiss_on_select: true,
                ..Default::default()
            },
            SelectionItem {
                name: "Quit".to_string(),
                description: None,
                actions: vec![Box::new(move |tx| {
                    quit_otel.counter(
                        "praxis.windows_sandbox.elevated_prompt_quit",
                        /*inc*/ 1,
                        &[],
                    );
                    tx.send(AppEvent::Exit(ExitMode::ShutdownFirst));
                })],
                dismiss_on_select: true,
                ..Default::default()
            },
        ];

        self.bottom_pane.show_selection_view(SelectionViewParams {
            title: None,
            footer_hint: Some(standard_popup_hint_line()),
            items,
            header: Box::new(header),
            ..Default::default()
        });
    }

    #[cfg(not(target_os = "windows"))]
    pub(crate) fn open_windows_sandbox_enable_prompt(&mut self, _preset: ApprovalPreset) {}

    #[cfg(target_os = "windows")]
    pub(crate) fn open_windows_sandbox_recovery_prompt(&mut self, preset: ApprovalPreset) {
        use ratatui_macros::line;

        let mut lines = Vec::new();
        lines.push(line![
            "Couldn't set up your sandbox with Administrator permissions".bold()
        ]);
        lines.push(line![""]);
        lines.push(line![
            "You can still use Praxis in a non-admin sandbox. It carries greater risk if prompt injected."
        ]);
        lines.push(line![
            "Learn more <https://github.com/o-oOvOo-o/praxis-backend/blob/main/docs/sandbox.md>"
        ]);

        let mut header = ColumnRenderable::new();
        header.push(*Box::new(Paragraph::new(lines).wrap(Wrap { trim: false })));

        let elevated_preset = preset.clone();
        let non_admin_preset = preset;
        let quit_otel = self.session_telemetry.clone();
        let items = vec![
            SelectionItem {
                name: "Try setting up admin sandbox again".to_string(),
                description: None,
                actions: vec![Box::new({
                    let otel = self.session_telemetry.clone();
                    let preset = elevated_preset;
                    move |tx| {
                        otel.counter(
                            "praxis.windows_sandbox.recovery_retry_elevated",
                            /*inc*/ 1,
                            &[],
                        );
                        tx.send(AppEvent::BeginWindowsSandboxElevatedSetup {
                            preset: preset.clone(),
                        });
                    }
                })],
                dismiss_on_select: true,
                ..Default::default()
            },
            SelectionItem {
                name: "Use Praxis with non-admin sandbox".to_string(),
                description: None,
                actions: vec![Box::new({
                    let otel = self.session_telemetry.clone();
                    let preset = non_admin_preset;
                    move |tx| {
                        otel.counter(
                            "praxis.windows_sandbox.recovery_use_non_admin",
                            /*inc*/ 1,
                            &[],
                        );
                        tx.send(AppEvent::BeginWindowsSandboxNonAdminSetup {
                            preset: preset.clone(),
                        });
                    }
                })],
                dismiss_on_select: true,
                ..Default::default()
            },
            SelectionItem {
                name: "Quit".to_string(),
                description: None,
                actions: vec![Box::new(move |tx| {
                    quit_otel.counter(
                        "praxis.windows_sandbox.recovery_prompt_quit",
                        /*inc*/ 1,
                        &[],
                    );
                    tx.send(AppEvent::Exit(ExitMode::ShutdownFirst));
                })],
                dismiss_on_select: true,
                ..Default::default()
            },
        ];

        self.bottom_pane.show_selection_view(SelectionViewParams {
            title: None,
            footer_hint: Some(standard_popup_hint_line()),
            items,
            header: Box::new(header),
            ..Default::default()
        });
    }

    #[cfg(not(target_os = "windows"))]
    pub(crate) fn open_windows_sandbox_recovery_prompt(&mut self, _preset: ApprovalPreset) {}

    #[cfg(target_os = "windows")]
    pub(crate) fn world_writable_warning_details(&self) -> Option<(Vec<String>, usize, bool)> {
        if self
            .config
            .notices
            .hide_world_writable_warning
            .unwrap_or(false)
        {
            return None;
        }
        let cwd = self.config.cwd.clone();
        let env_map: std::collections::HashMap<String, String> = std::env::vars().collect();
        match praxis_windows_sandbox::apply_world_writable_scan_and_denies(
            self.config.praxis_home.as_path(),
            cwd.as_path(),
            &env_map,
            self.config.permissions.sandbox_policy.get(),
            Some(self.config.praxis_home.as_path()),
        ) {
            Ok(_) => None,
            Err(_) => Some((Vec::new(), 0, true)),
        }
    }

    #[cfg(not(target_os = "windows"))]
    #[allow(dead_code)]
    pub(crate) fn world_writable_warning_details(&self) -> Option<(Vec<String>, usize, bool)> {
        None
    }

    #[cfg(target_os = "windows")]
    pub(crate) fn maybe_prompt_windows_sandbox_enable(&mut self, show_now: bool) {
        if show_now
            && WindowsSandboxLevel::from_config(&self.config) == WindowsSandboxLevel::Disabled
            && let Some(preset) = builtin_approval_presets()
                .into_iter()
                .find(|preset| preset.id == "auto")
        {
            self.open_windows_sandbox_enable_prompt(preset);
        }
    }

    #[cfg(not(target_os = "windows"))]
    pub(crate) fn maybe_prompt_windows_sandbox_enable(&mut self, _show_now: bool) {}

    #[cfg(target_os = "windows")]
    pub(crate) fn show_windows_sandbox_setup_status(&mut self) {
        self.bottom_pane.set_composer_input_enabled(
            /*enabled*/ false,
            Some("Input disabled until setup completes.".to_string()),
        );
        self.bottom_pane.ensure_status_indicator();
        self.bottom_pane
            .set_interrupt_hint_visible(/*visible*/ false);
        self.set_status(
            "Setting up sandbox...".to_string(),
            Some("Hang tight, this may take a few minutes".to_string()),
            StatusDetailsCapitalization::CapitalizeFirst,
            STATUS_DETAILS_DEFAULT_MAX_LINES,
        );
        self.request_redraw();
    }

    #[cfg(not(target_os = "windows"))]
    #[allow(dead_code)]
    pub(crate) fn show_windows_sandbox_setup_status(&mut self) {}

    #[cfg(target_os = "windows")]
    pub(crate) fn clear_windows_sandbox_setup_status(&mut self) {
        self.bottom_pane
            .set_composer_input_enabled(/*enabled*/ true, /*placeholder*/ None);
        self.bottom_pane.hide_status_indicator();
        self.request_redraw();
    }

    #[cfg(not(target_os = "windows"))]
    pub(crate) fn clear_windows_sandbox_setup_status(&mut self) {}
}
