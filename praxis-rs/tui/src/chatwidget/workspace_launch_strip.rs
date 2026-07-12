use praxis_config::types::ApprovalsReviewer;
#[cfg(target_os = "windows")]
use praxis_core::windows_sandbox::WindowsSandboxLevelExt;
use praxis_features::Feature;
use praxis_protocol::config_types::ModeKind;
#[cfg(target_os = "windows")]
use praxis_protocol::config_types::WindowsSandboxLevel;
use praxis_protocol::openai_models::ModelPreset;
use praxis_protocol::openai_models::ReasoningEffort as ReasoningEffortConfig;
use praxis_utils_approval_presets::builtin_approval_presets;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::text::Span;
use ratatui::widgets::Paragraph;
use ratatui::widgets::Widget;

use super::ChatWidget;
use super::LAUNCH_STRIP_RANK_MAX;
use super::WorkspaceReasoningChoice;
use super::surface_layout::ChatWidgetLayout;
use crate::app_event::AppEvent;
use crate::line_truncation::truncate_line_with_ellipsis_if_overflow;
use crate::text_formatting::truncate_text;
use crate::ui_language::UiLanguage;
use crate::workspace::LaunchStripDropdown;
use crate::workspace::LaunchStripDropdownItem;
use crate::workspace::LaunchStripDropdownMouseTarget;
use crate::workspace::LaunchStripMouseAction;
use crate::workspace::LaunchStripState;

impl ChatWidget {
    pub(crate) fn handle_workspace_chat_mouse_action(
        &mut self,
        launch: &mut LaunchStripState,
        action: LaunchStripMouseAction,
    ) {
        match action {
            LaunchStripMouseAction::ToggleModelDropdown => {
                if self.workspace_model_presets().is_empty() {
                    self.show_info_toast("Models are being updated; try again in a moment.");
                    launch.clear_dropdown();
                } else {
                    launch.toggle_dropdown(LaunchStripDropdown::Model);
                }
            }
            LaunchStripMouseAction::ToggleReasoningDropdown => {
                if self.workspace_reasoning_choices().is_empty() {
                    self.show_info_toast(
                        "Reasoning options are being updated; try again in a moment.",
                    );
                    launch.clear_dropdown();
                } else {
                    launch.toggle_dropdown(LaunchStripDropdown::Reasoning);
                }
            }
            LaunchStripMouseAction::ToggleRankDropdown => {
                launch.toggle_dropdown(LaunchStripDropdown::Rank);
            }
            LaunchStripMouseAction::TogglePermissionsDropdown => {
                launch.toggle_dropdown(LaunchStripDropdown::Permissions);
            }
            LaunchStripMouseAction::SelectModel(index) => {
                self.apply_workspace_model_dropdown_selection(launch, index);
            }
            LaunchStripMouseAction::SelectReasoning(index) => {
                self.apply_workspace_reasoning_dropdown_selection(launch, index);
            }
            LaunchStripMouseAction::SelectRank(rank) => {
                self.set_launch_strip_rank(launch, rank);
                launch.clear_dropdown();
            }
            LaunchStripMouseAction::SelectPermission(index) => {
                self.apply_workspace_permission_dropdown_selection(launch, index);
            }
            LaunchStripMouseAction::DismissDropdown => {
                launch.clear_dropdown();
            }
        }
        self.request_redraw();
    }

    pub(crate) fn set_launch_strip_rank(&mut self, launch: &mut LaunchStripState, rank: u8) {
        let next_rank = launch.set_rank(rank, LAUNCH_STRIP_RANK_MAX);
        let message = match self.ui_language {
            UiLanguage::En => format!(
                "Launch rank: R{} {}",
                next_rank,
                Self::workspace_rank_name(next_rank)
            ),
            UiLanguage::Cn => format!(
                "启动级别：R{} {}",
                next_rank,
                Self::workspace_rank_name(next_rank)
            ),
        };
        self.show_info_toast(message);
        self.request_redraw();
    }

    fn workspace_rank_name(rank: u8) -> &'static str {
        match rank {
            0 => "Coordinator",
            1 => "Supervisor",
            _ => "Worker",
        }
    }

    fn workspace_rank_description(rank: u8) -> &'static str {
        match rank {
            0 => "Top-level coordinator; can spawn and control R1/R2 descendants.",
            1 => "Supervisor; can coordinate direct R2 worker threads.",
            _ => "Worker; no cross-agent coordination role.",
        }
    }

    fn workspace_permissions_label(&self) -> String {
        if self.config.approvals_reviewer == ApprovalsReviewer::GuardianSubagent {
            return "Guardian Approvals".to_string();
        }
        let current_approval = self.config.permissions.approval_policy.value();
        let current_sandbox = self.config.permissions.sandbox_policy.get();
        builtin_approval_presets()
            .into_iter()
            .find(|preset| Self::preset_matches_current(current_approval, current_sandbox, preset))
            .map(|preset| preset.label.to_string())
            .unwrap_or_else(|| "Custom".to_string())
    }

    fn workspace_model_presets(&self) -> Vec<ModelPreset> {
        let models = self.model_catalog.try_list_models().unwrap_or_default();
        let mut indexed = models
            .into_iter()
            .enumerate()
            .filter(|(_, preset)| preset.show_in_picker)
            .collect::<Vec<_>>();
        indexed.sort_by_key(|(index, preset)| {
            let (group, order) = Self::model_picker_primary_rank(preset);
            (group, order, *index)
        });
        indexed.into_iter().map(|(_, preset)| preset).collect()
    }

    fn workspace_model_display_items(&self) -> Vec<LaunchStripDropdownItem> {
        self.workspace_model_presets()
            .into_iter()
            .map(|preset| {
                let selection = self.selection_metadata_or_current(&preset);
                let mut description = if preset.description.trim().is_empty() {
                    selection.provider_id.clone()
                } else {
                    preset.description.replace(" (Identical to Agent mode)", "")
                };
                if !selection.provider_id.is_empty()
                    && !description.contains(selection.provider_id.as_str())
                {
                    description.push_str("  ");
                    description.push_str(selection.provider_id.as_str());
                }
                LaunchStripDropdownItem {
                    name: Self::model_picker_item_name(&preset),
                    description: Some(description),
                    is_current: self.is_current_model_selection(&preset),
                    is_disabled: false,
                }
            })
            .collect()
    }

    fn apply_workspace_model_dropdown_selection(
        &mut self,
        launch: &mut LaunchStripState,
        index: usize,
    ) {
        let presets = self.workspace_model_presets();
        let Some(preset) = presets.get(index).cloned() else {
            launch.clear_dropdown();
            return;
        };
        let selection = self.selection_metadata_or_current(&preset);
        let model = preset.model.clone();
        let should_prompt_plan_mode_scope = self.should_prompt_plan_mode_reasoning_scope(
            model.as_str(),
            selection.provider_id.as_str(),
            Some(preset.default_reasoning_effort.clone()),
        );
        let actions = Self::model_selection_actions(
            model,
            selection.provider_id.clone(),
            Some(selection.provider.clone()),
            Some(preset.default_reasoning_effort),
            should_prompt_plan_mode_scope,
        );
        for action in actions {
            action(&self.app_event_tx);
        }
        launch.clear_dropdown();
    }

    fn workspace_current_model_preset(&self) -> Option<ModelPreset> {
        let models = self.model_catalog.try_list_models().ok()?;
        let current_model = self.current_model().to_string();
        let current_provider_id = self.current_model_provider_id().to_string();

        if let Some(index) = models.iter().position(|preset| {
            let selection = self.selection_metadata_or_current(preset);
            preset.model == current_model && selection.provider_id == current_provider_id
        }) {
            return models.get(index).cloned();
        }

        models
            .into_iter()
            .find(|preset| preset.model == current_model)
    }

    fn workspace_reasoning_explicit_effort(&self) -> Option<ReasoningEffortConfig> {
        if self.collaboration_modes_enabled() && self.active_mode_kind() == ModeKind::Plan {
            return self.config.plan_mode_reasoning_effort.clone();
        }
        self.current_collaboration_mode.reasoning_effort()
    }

    fn workspace_reasoning_display_label(&self) -> String {
        self.effective_reasoning_effort()
            .map(|effort| Self::reasoning_effort_label(&effort))
            .unwrap_or_else(|| "Default".to_string())
    }

    fn workspace_reasoning_effort_description(effort: &ReasoningEffortConfig) -> &'static str {
        match effort {
            ReasoningEffortConfig::None => "Disable explicit model thinking.",
            ReasoningEffortConfig::Minimal => "Use the smallest advertised thinking budget.",
            ReasoningEffortConfig::Low => "Use lightweight model thinking.",
            ReasoningEffortConfig::Medium => "Use balanced model thinking.",
            ReasoningEffortConfig::High => "Use deeper model thinking.",
            ReasoningEffortConfig::XHigh => "Use extra-high model thinking depth.",
            ReasoningEffortConfig::Max => "Use maximum single-agent reasoning depth.",
            ReasoningEffortConfig::Ultra => {
                "Use maximum reasoning with proactive multi-agent delegation."
            }
            ReasoningEffortConfig::Custom(_) => "Use this model-defined reasoning level.",
        }
    }

    fn workspace_reasoning_choices(&self) -> Vec<WorkspaceReasoningChoice> {
        let explicit_effort = self.workspace_reasoning_explicit_effort();
        let effective_effort = self.effective_reasoning_effort();
        let current_preset = self.workspace_current_model_preset();
        let default_display = effective_effort.or_else(|| {
            current_preset
                .as_ref()
                .map(|preset| preset.default_reasoning_effort.clone())
        });
        let default_name = default_display.map_or_else(
            || "Default".to_string(),
            |effort| {
                let label = current_preset.as_ref().map_or_else(
                    || Self::reasoning_effort_label(&effort),
                    |preset| Self::preset_reasoning_effort_label(preset, &effort),
                );
                format!("Default ({label})")
            },
        );
        let mut choices = vec![WorkspaceReasoningChoice {
            effort: None,
            name: default_name,
            description: Some("Use the current mode or model default reasoning level.".to_string()),
            is_current: explicit_effort.is_none(),
        }];

        if let Some(preset) = current_preset.as_ref() {
            for option in preset.supported_reasoning_efforts.iter() {
                let effort = option.effort.clone();
                if choices
                    .iter()
                    .any(|choice| choice.effort.as_ref() == Some(&effort))
                {
                    continue;
                }
                choices.push(WorkspaceReasoningChoice {
                    effort: Some(effort.clone()),
                    name: Self::preset_reasoning_effort_label(preset, &effort),
                    description: Some(option.description.clone())
                        .filter(|description| !description.trim().is_empty()),
                    is_current: explicit_effort.as_ref() == Some(&effort),
                });
            }
        }

        if choices.len() == 1 {
            for effort in ReasoningEffortConfig::known_values() {
                choices.push(WorkspaceReasoningChoice {
                    effort: Some(effort.clone()),
                    name: Self::reasoning_effort_label(&effort),
                    description: Some(
                        Self::workspace_reasoning_effort_description(&effort).to_string(),
                    ),
                    is_current: explicit_effort.as_ref() == Some(&effort),
                });
            }
        } else if let Some(effort) = explicit_effort
            && !choices
                .iter()
                .any(|choice| choice.effort.as_ref() == Some(&effort))
        {
            choices.push(WorkspaceReasoningChoice {
                effort: Some(effort.clone()),
                name: format!("{} (current)", Self::reasoning_effort_label(&effort)),
                description: Some(
                    "Current reasoning level is not advertised by the selected model.".to_string(),
                ),
                is_current: true,
            });
        }

        choices
    }

    fn workspace_reasoning_display_items(&self) -> Vec<LaunchStripDropdownItem> {
        self.workspace_reasoning_choices()
            .into_iter()
            .map(|choice| LaunchStripDropdownItem {
                name: choice.name,
                description: choice.description,
                is_current: choice.is_current,
                is_disabled: false,
            })
            .collect()
    }

    fn apply_workspace_reasoning_dropdown_selection(
        &mut self,
        launch: &mut LaunchStripState,
        index: usize,
    ) {
        let choices = self.workspace_reasoning_choices();
        let Some(choice) = choices.get(index).cloned() else {
            launch.clear_dropdown();
            return;
        };
        let effort = choice.effort;

        if self.collaboration_modes_enabled() && self.active_mode_kind() == ModeKind::Plan {
            self.app_event_tx
                .send(AppEvent::UpdatePlanModeReasoningEffort(effort.clone()));
            self.app_event_tx
                .send(AppEvent::PersistPlanModeReasoningEffort(effort));
        } else {
            self.app_event_tx.send(AppEvent::ApplyModelSelection {
                model: self.current_model().to_string(),
                provider_id: self.current_model_provider_id().to_string(),
                provider: Some(self.config.model_provider.clone()),
                effort,
            });
        }

        launch.clear_dropdown();
    }

    fn workspace_permission_display_items(&self) -> Vec<LaunchStripDropdownItem> {
        let include_read_only = cfg!(target_os = "windows");
        let current_approval = self.config.permissions.approval_policy.value();
        let current_sandbox = self.config.permissions.sandbox_policy.get();
        let current_review_policy = self.config.approvals_reviewer;
        let guardian_approval_enabled = self.config.features.enabled(Feature::GuardianApproval);

        #[cfg(target_os = "windows")]
        let windows_sandbox_level = WindowsSandboxLevel::from_config(&self.config);
        #[cfg(target_os = "windows")]
        let windows_degraded_sandbox_enabled =
            matches!(windows_sandbox_level, WindowsSandboxLevel::RestrictedToken);
        #[cfg(not(target_os = "windows"))]
        let windows_degraded_sandbox_enabled = false;

        let mut items = Vec::new();
        let guardian_disabled_reason = |enabled: bool| {
            let mut next_features = self.config.features.get().clone();
            next_features.set_enabled(Feature::GuardianApproval, enabled);
            self.config.features.can_set(&next_features).err()
        };

        for preset in builtin_approval_presets() {
            if !include_read_only && preset.id == "read-only" {
                continue;
            }
            let name = if preset.id == "auto" && windows_degraded_sandbox_enabled {
                "Default (non-admin sandbox)".to_string()
            } else {
                preset.label.to_string()
            };
            let description = Some(preset.description.replace(" (Identical to Agent mode)", ""));
            let approval_disabled = self
                .config
                .permissions
                .approval_policy
                .can_set(&preset.approval)
                .is_err();
            let default_disabled = approval_disabled || guardian_disabled_reason(false).is_some();
            let is_current = current_review_policy == ApprovalsReviewer::User
                && Self::preset_matches_current(current_approval, current_sandbox, &preset);
            items.push(LaunchStripDropdownItem {
                name: name.clone(),
                description: description.clone(),
                is_current,
                is_disabled: default_disabled,
            });

            if preset.id == "auto" && guardian_approval_enabled {
                items.push(LaunchStripDropdownItem {
                    name: "Guardian Approvals".to_string(),
                    description: Some(
                        "Same workspace-write permissions, approvals route through guardian."
                            .to_string(),
                    ),
                    is_current: current_review_policy == ApprovalsReviewer::GuardianSubagent
                        && Self::preset_matches_current(current_approval, current_sandbox, &preset),
                    is_disabled: approval_disabled || guardian_disabled_reason(true).is_some(),
                });
            }
        }

        items
    }

    fn apply_workspace_permission_dropdown_selection(
        &mut self,
        launch: &mut LaunchStripState,
        index: usize,
    ) {
        let mut model = self.permissions_menu_model();
        let Some(mut item) = (index < model.items.len()).then(|| model.items.remove(index)) else {
            launch.clear_dropdown();
            return;
        };
        if item.is_disabled || item.disabled_reason.is_some() {
            let reason = item
                .disabled_reason
                .unwrap_or_else(|| "permission preset is not available".to_string());
            self.show_error_toast(reason);
            launch.clear_dropdown();
            return;
        }

        for action in item.actions.drain(..) {
            action(&self.app_event_tx);
        }
        launch.clear_dropdown();
    }

    pub(super) fn render_launch_strip_strip(
        &self,
        layout: ChatWidgetLayout,
        buf: &mut Buffer,
        launch: &LaunchStripState,
    ) {
        self.clear_launch_strip_hit_areas(launch);
        if self.bottom_pane.has_active_view() {
            return;
        }
        let area = Self::workspace_input_strip_area(layout.bottom_outer_area);
        if area.is_empty() || area.height == 0 {
            return;
        }

        let row = Rect::new(area.x, area.y, area.width, 1);
        let theme = self.workspace_theme();
        let left_pad = 2.min(row.width.saturating_sub(1));
        let mut cursor_x = row.x.saturating_add(left_pad);
        let mut spans: Vec<Span<'static>> = vec![Span::raw(" ".repeat(left_pad as usize))];
        let mut has_chip = false;
        let mut push_chip = |chip: String, style: Style, target: &std::cell::Cell<Option<Rect>>| {
            if cursor_x >= row.right() {
                return;
            }
            if has_chip {
                let gap_width = 2;
                if row.right().saturating_sub(cursor_x) <= gap_width {
                    return;
                }
                spans.push(Span::styled("  ", Style::default()));
                cursor_x = cursor_x.saturating_add(gap_width);
            }

            let chip_width = u16::try_from(chip.chars().count()).unwrap_or(u16::MAX);
            if chip_width == 0 || cursor_x >= row.right() {
                return;
            }
            let visible_width = chip_width.min(row.right().saturating_sub(cursor_x));
            if visible_width == 0 {
                return;
            }
            target.set(Some(Rect::new(cursor_x, row.y, visible_width, 1)));
            spans.push(Span::styled(chip, style));
            cursor_x = cursor_x.saturating_add(chip_width);
            has_chip = true;
        };

        let model_label = truncate_text(self.model_display_name(), 24);
        let model_chip = format!(" {model_label} ▾ ");
        push_chip(
            model_chip,
            Style::default().fg(theme.text).bg(theme.chip_model_bg),
            &launch.model_area,
        );

        let reasoning_label = truncate_text(self.workspace_reasoning_display_label().as_str(), 12);
        let reasoning_chip = format!(" Reason {reasoning_label} ▾ ");
        push_chip(
            reasoning_chip,
            Style::default().fg(theme.text).bg(theme.chip_reasoning_bg),
            &launch.reasoning_area,
        );

        let rank_chip = format!(" R{} ▾ ", launch.rank);
        push_chip(
            rank_chip,
            Style::default()
                .fg(theme.accent)
                .bg(theme.chip_rank_bg)
                .add_modifier(Modifier::BOLD),
            &launch.rank_area,
        );

        let permission_label = self.workspace_permissions_label();
        let permission_label = truncate_text(permission_label.as_str(), 18);
        let permission_chip = format!(" {permission_label} ▾ ");
        push_chip(
            permission_chip,
            Style::default().fg(theme.text).bg(theme.chip_permission_bg),
            &launch.permissions_area,
        );

        let line = Line::from(spans);
        let line = truncate_line_with_ellipsis_if_overflow(line, row.width as usize);
        Paragraph::new(line).render(row, buf);
    }

    pub(super) fn render_launch_strip_dropdown(
        &self,
        layout: ChatWidgetLayout,
        buf: &mut Buffer,
        launch: &LaunchStripState,
    ) {
        launch.dropdown_targets.borrow_mut().clear();
        if self.bottom_pane.has_active_view() {
            return;
        }
        let Some(dropdown) = launch.dropdown else {
            return;
        };

        match dropdown {
            LaunchStripDropdown::Model => {
                let Some(anchor) = launch.model_area.get() else {
                    return;
                };
                self.render_workspace_dropdown_items(
                    layout,
                    buf,
                    anchor,
                    58,
                    self.workspace_model_display_items(),
                    launch,
                    LaunchStripMouseAction::SelectModel,
                );
            }
            LaunchStripDropdown::Reasoning => {
                let Some(anchor) = launch.reasoning_area.get() else {
                    return;
                };
                self.render_workspace_dropdown_items(
                    layout,
                    buf,
                    anchor,
                    42,
                    self.workspace_reasoning_display_items(),
                    launch,
                    LaunchStripMouseAction::SelectReasoning,
                );
            }
            LaunchStripDropdown::Rank => {
                let Some(anchor) = launch.rank_area.get() else {
                    return;
                };
                let items = (0..=LAUNCH_STRIP_RANK_MAX)
                    .map(|rank| LaunchStripDropdownItem {
                        name: format!("R{rank} {}", Self::workspace_rank_name(rank)),
                        description: Some(Self::workspace_rank_description(rank).to_string()),
                        is_current: launch.rank == rank,
                        is_disabled: false,
                    })
                    .collect::<Vec<_>>();
                self.render_workspace_dropdown_items(
                    layout,
                    buf,
                    anchor,
                    34,
                    items,
                    launch,
                    |index| LaunchStripMouseAction::SelectRank(index as u8),
                );
            }
            LaunchStripDropdown::Permissions => {
                let Some(anchor) = launch.permissions_area.get() else {
                    return;
                };
                self.render_workspace_dropdown_items(
                    layout,
                    buf,
                    anchor,
                    46,
                    self.workspace_permission_display_items(),
                    launch,
                    LaunchStripMouseAction::SelectPermission,
                );
            }
        }
    }

    fn render_workspace_dropdown_items(
        &self,
        layout: ChatWidgetLayout,
        buf: &mut Buffer,
        anchor: Rect,
        preferred_width: u16,
        items: Vec<LaunchStripDropdownItem>,
        launch: &LaunchStripState,
        action_for_index: impl Fn(usize) -> LaunchStripMouseAction,
    ) {
        if items.is_empty() {
            return;
        }

        let max_width = layout.bottom_outer_area.width.max(anchor.width).max(1);
        let width = preferred_width
            .max(anchor.width.saturating_add(2))
            .min(max_width);
        let height = u16::try_from(items.len())
            .unwrap_or(u16::MAX)
            .saturating_add(2);
        let x = anchor
            .x
            .min(layout.bottom_outer_area.right().saturating_sub(width));
        let below_y = anchor.y.saturating_add(1);
        let y = if below_y.saturating_add(height) <= layout.bottom_outer_area.bottom() {
            below_y
        } else {
            anchor.y.saturating_sub(height)
        };
        let area = Rect::new(x, y, width, height);
        if area.is_empty() {
            return;
        }

        let theme = self.workspace_theme();
        crate::surface::render_popup_surface(area, buf, theme, None);

        let inner_width = area.width.saturating_sub(2);
        for (index, item) in items.into_iter().enumerate() {
            let Ok(offset) = u16::try_from(index) else {
                break;
            };
            let row = Rect::new(
                area.x.saturating_add(1),
                area.y.saturating_add(1).saturating_add(offset),
                inner_width,
                1,
            );
            if row.y >= area.bottom().saturating_sub(1) {
                break;
            }
            let bg = if item.is_current {
                theme.dropdown_current_bg
            } else {
                theme.dropdown_bg
            };
            let fg = if item.is_disabled {
                theme.disabled
            } else if item.is_current {
                theme.accent
            } else {
                theme.text
            };
            let marker = if item.is_current { "● " } else { "  " };
            let mut line = Line::from(vec![
                Span::styled(marker, Style::default().fg(theme.accent).bg(bg)),
                Span::styled(item.name, Style::default().fg(fg).bg(bg)),
            ]);
            if let Some(description) = item.description {
                line.spans.push(Span::styled(
                    format!("  {description}"),
                    Style::default()
                        .fg(if item.is_disabled {
                            theme.disabled
                        } else {
                            theme.muted
                        })
                        .bg(bg),
                ));
            }
            buf.set_style(row, Style::default().bg(bg));
            truncate_line_with_ellipsis_if_overflow(line, row.width as usize).render(row, buf);

            launch
                .dropdown_targets
                .borrow_mut()
                .push(LaunchStripDropdownMouseTarget {
                    area: row,
                    action: action_for_index(index),
                });
        }
    }

    fn clear_launch_strip_hit_areas(&self, launch: &LaunchStripState) {
        launch.clear_hit_areas();
    }
}
